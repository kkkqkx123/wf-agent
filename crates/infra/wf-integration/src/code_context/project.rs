//! Project and index lifecycle: the wire shapes the runtime uses to make a
//! workspace exist in the service and be indexed, plus the state machine that
//! records how far that got.
//!
//! The workspace root is the single fact: the runtime lists the service's
//! projects, matches the normalized `root_path` exactly and creates the
//! project when it is missing. `default_project_id` is only an explicit
//! override, never the source of truth.
//!
//! Indexing is a long-running call with its own budget (it embeds the whole
//! workspace), so it does not reuse the per-call transport timeout. The
//! service already owns incremental updates through its own file watch, which
//! is why no incremental entry point is exposed here.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::transport::{get_json, post_json, ServiceError};

/// Project collection endpoint: list (`GET`) and create (`POST`).
pub const PROJECTS_ENDPOINT: &str = "/api/project";
/// Budget of one full index call. Indexing embeds the whole workspace, so it
/// is bounded by its own budget instead of the per-call timeout.
pub const FULL_INDEX_TIMEOUT_MS: u64 = 1_800_000;

/// One project record as the service reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectRecord {
    /// Numeric project id (the wire reports it as a decimal string).
    pub id: i64,
    /// Absolute project root as the service stores it.
    pub root_path: String,
    /// Last completed index time; `None` means never indexed.
    pub last_indexed: Option<String>,
}

/// How the workspace project is supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IndexSupplyMode {
    /// The deployment owns projects and indexes. The runtime only reads the
    /// configured default project id and never touches the watch.
    #[default]
    External,
    /// The runtime resolves the workspace project, triggers the first index
    /// and asks the service to watch for changes.
    Managed,
}

/// Project lifecycle as the runtime knows it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ProjectState {
    /// No runtime supply: the id comes from configuration only.
    #[default]
    NotSupplied,
    /// Not resolved yet (never attempted, or the last attempt failed).
    Unresolved { reason: String },
    /// Resolution in flight.
    Resolving,
    /// Resolved and the service is building the first index.
    Indexing { id: i64, started_at_ms: u64 },
    /// Resolved and indexed; retrieval can run.
    Ready {
        id: i64,
        last_indexed: Option<String>,
    },
    /// The last resolution or index attempt failed; the reason is kept until
    /// a manual retry replaces it. The project id survives the failure, so
    /// dead-letter maintenance and a manual reindex still know what they act
    /// on.
    Failed { reason: String, id: Option<i64> },
}

impl ProjectState {
    /// Short state name for diagnostics.
    pub fn name(&self) -> &'static str {
        match self {
            ProjectState::NotSupplied => "not_supplied",
            ProjectState::Unresolved { .. } => "unresolved",
            ProjectState::Resolving => "resolving",
            ProjectState::Indexing { .. } => "indexing",
            ProjectState::Ready { .. } => "ready",
            ProjectState::Failed { .. } => "failed",
        }
    }

    /// Resolved project id, when one is known.
    pub fn id(&self) -> Option<i64> {
        match self {
            ProjectState::Indexing { id, .. }
            | ProjectState::Ready { id, .. }
            | ProjectState::Failed { id: Some(id), .. } => Some(*id),
            _ => None,
        }
    }
}

/// Point-in-time project view for diagnostics.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectStatus {
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_indexed: Option<String>,
    /// How long the current index run has been going, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexing_for_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl From<&ProjectState> for ProjectStatus {
    fn from(state: &ProjectState) -> Self {
        match state {
            ProjectState::NotSupplied => ProjectStatus {
                state: "not_supplied",
                project_id: None,
                last_indexed: None,
                indexing_for_ms: None,
                reason: None,
            },
            ProjectState::Unresolved { reason } | ProjectState::Failed { reason, .. } => {
                ProjectStatus {
                    state: state.name(),
                    project_id: state.id(),
                    last_indexed: None,
                    indexing_for_ms: None,
                    reason: Some(reason.clone()),
                }
            }
            ProjectState::Resolving => ProjectStatus {
                state: "resolving",
                project_id: None,
                last_indexed: None,
                indexing_for_ms: None,
                reason: None,
            },
            ProjectState::Indexing { id, started_at_ms } => ProjectStatus {
                state: "indexing",
                project_id: Some(*id),
                last_indexed: None,
                indexing_for_ms: Some(elapsed_ms(*started_at_ms)),
                reason: None,
            },
            ProjectState::Ready { id, last_indexed } => ProjectStatus {
                state: "ready",
                project_id: Some(*id),
                last_indexed: last_indexed.clone(),
                indexing_for_ms: None,
                reason: None,
            },
        }
    }
}

fn elapsed_ms(started_at_ms: u64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    now.saturating_sub(started_at_ms)
}

/// Milliseconds since the UNIX epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Canonical form of a project root for matching: absolute, symlink-resolved
/// and case-folded on case-insensitive platforms. The service may have
/// created the project from an equivalent but differently spelled path, so
/// matching on the raw string would duplicate projects.
pub fn normalize_root_path(path: &Path) -> String {
    let absolute: PathBuf = std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    });
    let text = absolute.to_string_lossy().replace('\\', "/");
    let trimmed = text.trim_end_matches('/').to_string();
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed
    }
}

/// Body of one project create request. The name is derived from the root
/// directory so the service list stays readable.
pub fn create_project_body(root_path: &str) -> serde_json::Value {
    let name = Path::new(root_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "workspace".to_string());
    serde_json::json!({ "name": name, "root_path": root_path })
}

/// Decode a numeric id the service may spell as a JSON number or as a
/// decimal string. Both spellings appear in the same service, so one rule
/// covers project ids and dead-letter project ids.
pub fn decimal_i64(value: &serde_json::Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        value
            .as_str()
            .map(str::trim)
            .and_then(|s| s.parse::<i64>().ok())
    })
}

fn parse_record(value: &serde_json::Value) -> Result<ProjectRecord, ServiceError> {
    let id = value.get("id").and_then(decimal_i64).ok_or_else(|| {
        ServiceError::protocol("project record carries no numeric 'id' (expected a decimal string)")
    })?;
    let root_path = value
        .get("root_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServiceError::protocol("project record carries no 'root_path'"))?
        .to_string();
    let last_indexed = value
        .get("last_indexed")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Ok(ProjectRecord {
        id,
        root_path,
        last_indexed,
    })
}

/// Decode a project list response.
pub fn parse_projects(payload: &serde_json::Value) -> Result<Vec<ProjectRecord>, ServiceError> {
    let items = payload
        .get("projects")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ServiceError::protocol(
                "project list response carries no 'projects' array; the service contract is not the expected one",
            )
        })?;
    items.iter().map(parse_record).collect()
}

/// Decode a single project response.
pub fn parse_project(payload: &serde_json::Value) -> Result<ProjectRecord, ServiceError> {
    match payload.get("project") {
        Some(value) => parse_record(value),
        None => parse_record(payload),
    }
}

/// Match a project record to a workspace root, comparing normalized paths so
/// symlinks and case differences collapse onto one project.
pub fn find_by_root<'a>(projects: &'a [ProjectRecord], root: &Path) -> Option<&'a ProjectRecord> {
    let wanted = normalize_root_path(root);
    projects
        .iter()
        .find(|project| normalize_root_path(Path::new(&project.root_path)) == wanted)
}

/// List the service's projects.
pub async fn list_projects(
    client: &reqwest::Client,
    base_url: &str,
    timeout_ms: u64,
) -> Result<Vec<ProjectRecord>, ServiceError> {
    let payload = get_json(
        client,
        &format!("{base_url}{PROJECTS_ENDPOINT}"),
        timeout_ms,
    )
    .await?;
    parse_projects(&payload)
}

/// Create a project for `root_path`.
pub async fn create_project(
    client: &reqwest::Client,
    base_url: &str,
    timeout_ms: u64,
    root_path: &str,
) -> Result<ProjectRecord, ServiceError> {
    let payload = post_json(
        client,
        &format!("{base_url}{PROJECTS_ENDPOINT}"),
        timeout_ms,
        &create_project_body(root_path),
    )
    .await?;
    parse_project(&payload)
}

/// Trigger the first full index of a project. Long-running by nature, hence
/// its own budget.
pub async fn trigger_full_index(
    client: &reqwest::Client,
    base_url: &str,
    project_id: i64,
) -> Result<(), ServiceError> {
    post_json(
        client,
        &format!("{base_url}{PROJECTS_ENDPOINT}/{project_id}/index"),
        FULL_INDEX_TIMEOUT_MS,
        &serde_json::json!({}),
    )
    .await
    .map(|_| ())
}

/// Start the service-side file watch that keeps the index current.
pub async fn start_watch(
    client: &reqwest::Client,
    base_url: &str,
    project_id: i64,
    timeout_ms: u64,
) -> Result<(), ServiceError> {
    post_json(
        client,
        &format!("{base_url}{PROJECTS_ENDPOINT}/{project_id}/watch/start"),
        timeout_ms,
        &serde_json::json!({}),
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, root: &str, last_indexed: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "root_path": root,
            "last_indexed": last_indexed,
        })
    }

    #[test]
    fn project_list_decodes_decimal_string_ids() {
        let projects = parse_projects(&serde_json::json!({
            "projects": [record("7", "/work/a", None), record("9", "/work/b", Some("2024-05-01T00:00:00Z"))]
        }))
        .expect("decode");
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].id, 7);
        assert!(projects[0].last_indexed.is_none());
        assert_eq!(
            projects[1].last_indexed.as_deref(),
            Some("2024-05-01T00:00:00Z")
        );
    }

    #[test]
    fn missing_projects_array_is_a_protocol_error() {
        let error = parse_projects(&serde_json::json!({ "items": [] })).unwrap_err();
        assert_eq!(error.kind, crate::transport::ServiceErrorKind::Protocol);
        assert!(error.reason.contains("projects"), "{error}");
    }

    #[test]
    fn project_create_body_names_the_root_directory() {
        let body = create_project_body("/work/my-repo");
        assert_eq!(body["name"], serde_json::json!("my-repo"));
        assert_eq!(body["root_path"], serde_json::json!("/work/my-repo"));
    }

    #[test]
    fn root_matching_normalizes_trailing_slash_and_symlinks() {
        let dir = std::env::temp_dir().join(format!("wf-cce-root-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let projects = vec![ProjectRecord {
            id: 1,
            root_path: dir.to_string_lossy().into_owned(),
            last_indexed: None,
        }];
        assert!(find_by_root(&projects, &dir).is_some());
        let with_slash = PathBuf::from(format!("{}/", dir.to_string_lossy()));
        assert!(find_by_root(&projects, &with_slash).is_some());
        assert!(find_by_root(&projects, &dir.join("other")).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
