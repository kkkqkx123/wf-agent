use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::transport::SidecarSpec;

pub mod dead_letters;
pub mod fold;
pub mod navigation;
pub mod project;
pub mod retrieval;
pub mod service;

pub use dead_letters::{
    acknowledge, dead_letters_acknowledge_path, dead_letters_path, dead_letters_retry_path,
    list as list_dead_letters, parse_action, parse_dead_letters, retry as retry_dead_letters,
    DeadLetterAction, DeadLetterFile, DeadLetterList, DeadLetterModule,
    DEAD_LETTER_RETRY_TIMEOUT_MS,
};
pub use project::{
    create_project_body, decimal_i64, find_by_root, list_projects, normalize_root_path,
    parse_project, parse_projects, start_watch, trigger_full_index, IndexSupplyMode, ProjectRecord,
    ProjectState, ProjectStatus, PROJECTS_ENDPOINT,
};
pub use service::{
    CallScope, CapabilityReport, CodeContextAvailability, CodeContextService, CodeContextStatus,
    OperationError, ReconfigureOutcome, SharedCodeContextService,
};

/// Fold policy for the compression-path file folding stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoldPolicy {
    /// Tool-result messages below this estimated token count are never
    /// sent for folding.
    pub min_tokens: usize,
    /// Per-entry token budget requested from the fold service.
    pub max_tokens: usize,
    /// Maximum entries per batch request (local send intent; the service
    /// clamps to its own limit and rejects oversized batches).
    pub max_items: usize,
    /// Maximum retry rounds for failed batches; failed chunks are split
    /// and retried up to this many times before being abandoned.
    pub max_retries: u32,
    /// Wall-clock budget for the whole fold stage. Exhausting it ends the
    /// stage with a skip instead of holding the compression chain open.
    pub total_budget_ms: u64,
}

impl Default for FoldPolicy {
    fn default() -> Self {
        Self {
            min_tokens: 800,
            max_tokens: 2000,
            max_items: 32,
            max_retries: 2,
            total_budget_ms: 60_000,
        }
    }
}

/// Retrieval policy for the predefined code search tools.
///
/// `default_limit` and `max_results` are caller-intent budgets: how much
/// context the caller can afford per call. The service remains the final
/// enforcer and clamps to its own hard limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalPolicy {
    /// Explicit project override when the call names none. Not the source of
    /// truth: with runtime supply the workspace project resolved from the
    /// service wins, and this only applies to deployments that own their
    /// projects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_project_id: Option<i64>,
    /// Default result count when the call does not name one.
    pub default_limit: usize,
    /// Caller-side ceiling; the service clamps to its own limit beyond it.
    pub max_results: usize,
}

impl Default for RetrievalPolicy {
    fn default() -> Self {
        Self {
            default_project_id: None,
            default_limit: 10,
            max_results: 100,
        }
    }
}

/// Project and index supply policy.
///
/// `supply` is the explicit answer to "who owns the workspace project": the
/// runtime (managed) or the deployment (external). The config transform
/// derives it from the transport mode when the key is absent, so the runtime
/// never has to guess.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectPolicy {
    /// Who resolves and indexes the workspace project.
    pub supply: IndexSupplyMode,
    /// Managed supply keeps the index current through the service-side file
    /// watch. The external mode never touches the watch.
    pub managed_watch: bool,
    /// Timeout of one watch start/stop call in milliseconds.
    pub watch_timeout_ms: u64,
}

impl Default for ProjectPolicy {
    fn default() -> Self {
        Self {
            supply: IndexSupplyMode::External,
            managed_watch: true,
            watch_timeout_ms: 10_000,
        }
    }
}

/// Validated code-context service config with defaults applied.
///
/// Lives in `tools.toml` under `[code_context]` (flat keys; the transform
/// layer maps them onto the structs below) and serves every channel: the
/// compression-path file folding stage, the predefined retrieval tools and the
/// workspace project/index supply. Absent entirely means the service is
/// disabled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CodeContextConfig {
    /// Master switch. Folding and retrieval tools stay inert while false,
    /// even when the remaining fields carry values.
    pub enabled: bool,
    /// How the service is reached; owned by the transport module.
    pub transport: super::transport::ServiceTransport,
    /// Fold channel policy.
    pub fold: FoldPolicy,
    /// Retrieval channel policy.
    pub retrieval: RetrievalPolicy,
    /// Project and index supply policy.
    pub project: ProjectPolicy,
}

/// Write a minimal code-context server config: loopback address plus every
/// store pinned under `data_dir`. The server merges the file over its
/// defaults, so only the paths that must not float are written here.
///
/// Paths are written absolute because the server anchors relative paths at
/// the config file's directory, which lives in the system temp directory.
pub fn write_cce_server_config(host: &str, port: u16, data_dir: &Path) -> std::io::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "wf-cce-server-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let absolute = |name: &str| data_dir.join(name).to_string_lossy().into_owned();
    std::fs::write(
        &path,
        format!(
            "[server]\nhost = \"{host}\"\nport = {port}\n\n\
             [database.sqlite]\npath = \"{sqlite}\"\n\n\
             [database.vector_local]\ndata_dir = \"{vectors}\"\n\n\
             [database.bm25]\nindex_path = \"{bm25}\"\n\n\
             [logger]\nfile = \"{log}\"\n",
            sqlite = absolute("metadata.db"),
            vectors = absolute("vectors"),
            bm25 = absolute("bm25"),
            log = absolute("cce.log"),
        ),
    )?;
    Ok(path)
}

/// Launch line for a code-context HTTP server: config through the
/// `CCE_CONFIG` environment value, no CLI arguments needed. The log level is
/// an environment override, so an unrecognized service version ignores it
/// instead of refusing to start.
pub fn cce_server_command(
    binary: &str,
    config_path: &Path,
    log_level: &str,
) -> (String, Vec<String>, Vec<(String, String)>) {
    let mut env = vec![(
        "CCE_CONFIG".to_string(),
        config_path.to_string_lossy().into_owned(),
    )];
    let level = log_level.trim();
    if !level.is_empty() {
        env.push(("RUST_LOG".to_string(), level.to_string()));
    }
    (binary.to_string(), Vec::new(), env)
}

/// Prepare the managed service: data layout, pinned server config and the
/// launch line. The data directory lock is not taken here: it belongs to the
/// supervisor, which holds it for the whole supervised lifetime (including
/// retries) and releases it when it stops for good.
pub fn prepare_managed_service(
    config: &CodeContextConfig,
    port: u16,
    data_dir: &Path,
) -> Result<SidecarSpec, String> {
    std::fs::create_dir_all(data_dir).map_err(|e| {
        format!(
            "code-context data directory not creatable at {}: {e}",
            data_dir.display()
        )
    })?;
    let config_path = write_cce_server_config("127.0.0.1", port, data_dir)
        .map_err(|e| format!("managed service config not writable: {e}"))?;
    let (program, args, env) = cce_server_command(
        &config.transport.managed_binary,
        &config_path,
        &config.transport.managed_log_level,
    );
    let base_url = format!("http://127.0.0.1:{port}");
    Ok(SidecarSpec {
        program,
        args,
        env,
        base_url,
        startup_timeout_ms: config.transport.managed_startup_timeout_ms,
        max_restarts: config.transport.managed_max_restarts,
        shutdown_grace_ms: 5_000,
        temp_files: vec![config_path],
        exclusive_lock: Some(data_dir.join("runtime.lock")),
        log_file: Some(data_dir.join("cce.log")),
        log_tail_lines: 20,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportMode;

    #[test]
    fn cce_server_config_pins_loopback_address_and_stores() {
        let data_dir = std::env::temp_dir().join(format!("wf-cce-config-{}", std::process::id()));
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        let path =
            write_cce_server_config("127.0.0.1", 9123, &data_dir).expect("write temp config");
        let content = std::fs::read_to_string(&path).expect("read temp config");
        assert!(content.contains("127.0.0.1"));
        assert!(content.contains("9123"));
        assert!(content.contains(&data_dir.join("metadata.db").to_string_lossy().into_owned()));
        assert!(!content.contains("path = \"metadata.db\""));
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir_all(&data_dir).ok();
    }

    #[test]
    fn managed_service_files_are_prepared_for_the_supervisor() {
        let data_dir = std::env::temp_dir().join(format!("wf-cce-managed-{}", std::process::id()));
        std::fs::remove_dir_all(&data_dir).ok();
        let config = CodeContextConfig {
            enabled: true,
            transport: super::super::transport::ServiceTransport {
                transport_mode: TransportMode::Managed,
                ..Default::default()
            },
            ..Default::default()
        };
        let spec = prepare_managed_service(&config, 9321, &data_dir).expect("prepare managed");
        assert_eq!(spec.base_url, "http://127.0.0.1:9321");
        // The config file is caller-owned; the data directory lock and the
        // log belong to the supervisor.
        assert_eq!(spec.temp_files.len(), 1);
        assert_eq!(spec.exclusive_lock, Some(data_dir.join("runtime.lock")));
        assert_eq!(spec.log_file, Some(data_dir.join("cce.log")));
        assert!(spec.env.iter().any(|(key, _)| key == "CCE_CONFIG"));

        for path in &spec.temp_files {
            std::fs::remove_file(path).ok();
        }
        std::fs::remove_dir_all(&data_dir).ok();
    }
}
