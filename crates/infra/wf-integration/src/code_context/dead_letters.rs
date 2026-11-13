//! Dead-letter channel: files the service gave up on, plus the two operator
//! actions over them.
//!
//! A dead letter is per file and per pipeline module (relation, summary,
//! embedding, bm25, export). It is not retried automatically: the runtime
//! exposes read plus manual retry/acknowledge and nothing else, so a broken
//! embedder cannot turn into a retry storm.
//!
//! Retry re-runs the pipeline modules for the selected files, so it carries
//! its own budget instead of the per-call transport timeout.

use serde::{Deserialize, Serialize};

use super::project::{decimal_i64, PROJECTS_ENDPOINT};
use crate::transport::{get_json, post_json, ServiceError};

/// Budget of one dead-letter retry pass: it re-parses and re-embeds the
/// affected files, which is closer to an index run than to a query.
pub const DEAD_LETTER_RETRY_TIMEOUT_MS: u64 = 600_000;

/// One pipeline module that gave up on a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterModule {
    /// Module name as the service reports it.
    pub module: String,
    /// Retry attempts already made before the module went dead.
    pub retry_count: u32,
    /// Stable error code of the last failure, when the service knows one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Human-readable failure message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Whether the module already went through a lossy truncate-retry.
    pub truncated: bool,
    /// Acknowledged modules no longer participate in retry passes.
    pub acknowledged: bool,
}

/// One dead-lettered file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterFile {
    /// Recorded file path as the service stores it.
    pub file_path: String,
    /// Index version the dead letter belongs to.
    pub version: u64,
    /// Modules currently dead for this file.
    pub modules: Vec<DeadLetterModule>,
    /// Last update time as the service reports it.
    pub updated_at: String,
}

/// Deadline list of one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterList {
    pub project_id: i64,
    pub files: Vec<DeadLetterFile>,
}

impl DeadLetterList {
    /// Modules across every file that would still be retried. Acknowledged
    /// entries are excluded because the service skips them too.
    pub fn pending_modules(&self) -> usize {
        self.files
            .iter()
            .map(|file| {
                file.modules
                    .iter()
                    .filter(|module| !module.acknowledged)
                    .count()
            })
            .sum()
    }
}

/// Outcome of one operator action over dead letters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterAction {
    /// Whether the service carried the action out. A refusal is a legitimate
    /// answer (for example "nothing to retry"), not a transport failure.
    pub success: bool,
    /// Files the action touched.
    pub affected: usize,
    /// Service-supplied summary, kept verbatim for the operator.
    pub message: String,
}

/// Path of the dead-letter list endpoint for one project.
pub fn dead_letters_path(project_id: i64) -> String {
    format!("{PROJECTS_ENDPOINT}/{project_id}/dead-letters")
}

/// Path of the retry endpoint for one project.
pub fn dead_letters_retry_path(project_id: i64) -> String {
    format!("{}/retry", dead_letters_path(project_id))
}

/// Path of the acknowledge endpoint for one project.
pub fn dead_letters_acknowledge_path(project_id: i64) -> String {
    format!("{}/acknowledge", dead_letters_path(project_id))
}

/// Body of a retry request. An empty file list means "every candidate".
pub fn retry_body(files: &[String]) -> serde_json::Value {
    serde_json::json!({ "files": files })
}

/// Body of an acknowledge request.
pub fn acknowledge_body(file_path: &str, module: Option<&str>) -> serde_json::Value {
    match module {
        Some(module) => serde_json::json!({ "file_path": file_path, "module": module }),
        None => serde_json::json!({ "file_path": file_path }),
    }
}

fn protocol(reason: impl Into<String>) -> ServiceError {
    ServiceError::protocol(reason)
}

fn parse_module(value: &serde_json::Value) -> Result<DeadLetterModule, ServiceError> {
    let module = value
        .get("module")
        .and_then(|v| v.as_str())
        .ok_or_else(|| protocol("dead-letter module entry carries no 'module' name"))?;
    let retry_count = value
        .get("retry_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    Ok(DeadLetterModule {
        module: module.to_string(),
        retry_count,
        error_code: value
            .get("error_code")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        error_message: value
            .get("error_message")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        truncated: value
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        acknowledged: value
            .get("acknowledged")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

fn parse_file(value: &serde_json::Value) -> Result<DeadLetterFile, ServiceError> {
    let file_path = value
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| protocol("dead-letter file entry carries no 'file_path'"))?;
    let version = value
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| protocol("dead-letter file entry carries no 'version'"))?;
    let modules = value
        .get("modules")
        .and_then(|v| v.as_array())
        .ok_or_else(|| protocol("dead-letter file entry carries no 'modules' array"))?;
    Ok(DeadLetterFile {
        file_path: file_path.to_string(),
        version,
        modules: modules
            .iter()
            .map(parse_module)
            .collect::<Result<Vec<_>, _>>()?,
        updated_at: value
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

/// Decode a dead-letter list response.
pub fn parse_dead_letters(payload: &serde_json::Value) -> Result<DeadLetterList, ServiceError> {
    let project_id = payload
        .get("project_id")
        .and_then(decimal_i64)
        .ok_or_else(|| protocol("dead-letter list response carries no numeric 'project_id'"))?;
    let files = payload
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            protocol(
                "dead-letter list response carries no 'files' array; the service contract is not the expected one",
            )
        })?;
    Ok(DeadLetterList {
        project_id,
        files: files
            .iter()
            .map(parse_file)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

/// Decode a dead-letter action response.
pub fn parse_action(payload: &serde_json::Value) -> Result<DeadLetterAction, ServiceError> {
    let success = payload
        .get("success")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| protocol("dead-letter action response carries no 'success' flag"))?;
    Ok(DeadLetterAction {
        success,
        affected: payload
            .get("affected")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        message: payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

/// Read the project's dead letters.
pub async fn list(
    client: &reqwest::Client,
    base_url: &str,
    project_id: i64,
    timeout_ms: u64,
) -> Result<DeadLetterList, ServiceError> {
    let payload = get_json(
        client,
        &format!("{base_url}{}", dead_letters_path(project_id)),
        timeout_ms,
    )
    .await?;
    parse_dead_letters(&payload)
}

/// Retry dead letters: `files` empty means every candidate.
pub async fn retry(
    client: &reqwest::Client,
    base_url: &str,
    project_id: i64,
    files: &[String],
) -> Result<DeadLetterAction, ServiceError> {
    let payload = post_json(
        client,
        &format!("{base_url}{}", dead_letters_retry_path(project_id)),
        DEAD_LETTER_RETRY_TIMEOUT_MS,
        &retry_body(files),
    )
    .await?;
    parse_action(&payload)
}

/// Acknowledge one dead letter, for one module when named.
pub async fn acknowledge(
    client: &reqwest::Client,
    base_url: &str,
    project_id: i64,
    file_path: &str,
    module: Option<&str>,
    timeout_ms: u64,
) -> Result<DeadLetterAction, ServiceError> {
    let payload = post_json(
        client,
        &format!("{base_url}{}", dead_letters_acknowledge_path(project_id)),
        timeout_ms,
        &acknowledge_body(file_path, module),
    )
    .await?;
    parse_action(&payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_letter_list_decodes_modules() {
        let list = parse_dead_letters(&serde_json::json!({
            "project_id": 7,
            "files": [{
                "file_path": "src/big.rs",
                "version": 3,
                "updated_at": "2024-05-01T00:00:00Z",
                "modules": [
                    {"module": "embedding", "retry_count": 4, "error_code": "EMBED_FAIL", "error_message": "model unavailable", "truncated": true, "acknowledged": false},
                    {"module": "summary", "retry_count": 1, "truncated": false, "acknowledged": true}
                ]
            }]
        }))
        .expect("decode");
        assert_eq!(list.project_id, 7);
        assert_eq!(list.files.len(), 1);
        assert_eq!(list.files[0].version, 3);
        assert_eq!(list.files[0].modules[0].module, "embedding");
        assert_eq!(list.files[0].modules[0].retry_count, 4);
        assert_eq!(
            list.files[0].modules[0].error_message.as_deref(),
            Some("model unavailable")
        );
        assert!(list.files[0].modules[0].truncated);
        assert!(list.files[0].modules[1].acknowledged);
        assert!(list.files[0].modules[1].error_code.is_none());
        // Acknowledged modules no longer count as work.
        assert_eq!(list.pending_modules(), 1);
    }

    #[test]
    fn empty_dead_letter_list_is_valid() {
        let list = parse_dead_letters(&serde_json::json!({ "project_id": "9", "files": [] }))
            .expect("decode");
        assert_eq!(list.project_id, 9);
        assert!(list.files.is_empty());
        assert_eq!(list.pending_modules(), 0);
    }

    #[test]
    fn missing_files_array_is_a_protocol_error() {
        let error = parse_dead_letters(&serde_json::json!({ "project_id": 1 })).unwrap_err();
        assert!(error.reason.contains("files"), "{error}");
    }

    #[test]
    fn file_without_modules_is_a_protocol_error() {
        let error = parse_dead_letters(&serde_json::json!({
            "project_id": 1,
            "files": [{ "file_path": "a.rs", "version": 1 }]
        }))
        .unwrap_err();
        assert!(error.reason.contains("modules"), "{error}");
    }

    #[test]
    fn refused_action_stays_a_decodable_answer() {
        let action = parse_action(&serde_json::json!({
            "success": false,
            "affected": 0,
            "message": "no dead letters to retry"
        }))
        .expect("decode");
        assert!(!action.success);
        assert_eq!(action.message, "no dead letters to retry");
    }

    #[test]
    fn request_bodies_match_the_service_contract() {
        assert_eq!(
            retry_body(&["a.rs".to_string()]),
            serde_json::json!({ "files": ["a.rs"] })
        );
        assert_eq!(retry_body(&[]), serde_json::json!({ "files": [] }));
        assert_eq!(
            acknowledge_body("a.rs", Some("embedding")),
            serde_json::json!({ "file_path": "a.rs", "module": "embedding" })
        );
        assert_eq!(
            acknowledge_body("a.rs", None),
            serde_json::json!({ "file_path": "a.rs" })
        );
    }

    #[test]
    fn paths_are_project_scoped() {
        assert_eq!(dead_letters_path(7), "/api/project/7/dead-letters");
        assert_eq!(
            dead_letters_retry_path(7),
            "/api/project/7/dead-letters/retry"
        );
        assert_eq!(
            dead_letters_acknowledge_path(7),
            "/api/project/7/dead-letters/acknowledge"
        );
    }
}
