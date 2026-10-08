use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::transport::ServiceTransport;

pub mod fold;
pub mod navigation;
pub mod retrieval;

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
}

impl Default for FoldPolicy {
    fn default() -> Self {
        Self {
            min_tokens: 800,
            max_tokens: 2000,
            max_items: 32,
            max_retries: 2,
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
    /// Default project id for the stateful retrieval tools when the call
    /// does not name one.
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

/// Validated code-context service config with defaults applied.
///
/// Lives in `tools.toml` under `[code_context]` (flat keys; the transform
/// layer maps them onto the structs below) and serves both channels: the
/// compression-path file folding stage and the predefined retrieval tools.
/// Absent entirely means the service is disabled; every consumer treats
/// `None` (or `enabled == false`) as "skip silently".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CodeContextConfig {
    /// Master switch. Folding and retrieval tools stay inert while false,
    /// even when the remaining fields carry values.
    pub enabled: bool,
    /// How the service is reached; owned by the transport module.
    pub transport: ServiceTransport,
    /// Fold channel policy.
    pub fold: FoldPolicy,
    /// Retrieval channel policy.
    pub retrieval: RetrievalPolicy,
}

impl CodeContextConfig {
    /// Whether the service may be contacted: switched on with a reachable
    /// transport (an address for external mode, a binary for managed mode).
    pub fn is_usable(&self) -> bool {
        self.enabled && self.transport.is_usable()
    }

    /// Direct-call base URL. Only meaningful for external transport;
    /// managed transport resolves to an equivalent external config once
    /// the supervised process reports its address.
    pub fn external_base_url(&self) -> Option<String> {
        if !self.enabled {
            return None;
        }
        self.transport.external_base_url()
    }

    /// Resolve managed transport into an equivalent external config once
    /// the supervised process reports its address. Downstream consumers
    /// only ever observe external configs.
    pub fn with_managed_address(&self, base_url: String) -> Self {
        Self {
            transport: self.transport.with_managed_address(base_url),
            ..self.clone()
        }
    }
}

/// Write a minimal code-context server config binding loopback. The
/// server merges the file over its defaults, so only the address needs
/// pinning here. Returns the config path for the `CCE_CONFIG` env value.
pub fn write_cce_server_config(host: &str, port: u16) -> std::io::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "wf-cce-server-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(
        &path,
        format!("[server]\nhost = \"{host}\"\nport = {port}\n"),
    )?;
    Ok(path)
}

/// Launch line for a code-context HTTP server: config through the
/// `CCE_CONFIG` environment value, no CLI arguments needed.
pub fn cce_server_command(
    binary: &str,
    config_path: &Path,
) -> (String, Vec<String>, Vec<(String, String)>) {
    (
        binary.to_string(),
        Vec::new(),
        vec![(
            "CCE_CONFIG".to_string(),
            config_path.to_string_lossy().into_owned(),
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cce_server_config_pins_loopback_address() {
        let path = write_cce_server_config("127.0.0.1", 9123).expect("write temp config");
        let content = std::fs::read_to_string(&path).expect("read temp config");
        assert!(content.contains("127.0.0.1"));
        assert!(content.contains("9123"));
        std::fs::remove_file(&path).ok();
    }
}
