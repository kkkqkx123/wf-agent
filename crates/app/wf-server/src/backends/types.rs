//! Backend registry data model: persisted reference config plus
//! runtime liveness that is never persisted.
//!
//! Field names mirror the CLI registry file so one-time import is a direct
//! copy; the server adds runtime fields only in views.

use serde::{Deserialize, Serialize};

/// Persisted registry entry: connection references only, never key material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BackendEntry {
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub key_path: Option<String>,
    pub jump_host: Option<String>,
    pub remote_dir: Option<String>,
    pub server_port: Option<u16>,
    pub local_port: Option<u16>,
    #[serde(default)]
    pub backup_interval_secs: Option<u64>,
    #[serde(default)]
    pub backup_retention_count: Option<usize>,
    #[serde(default)]
    pub backup_retention_days: Option<u64>,
    #[serde(default)]
    pub backup_executions: Vec<String>,
    #[serde(default)]
    pub backup_directory: Option<String>,
}

impl BackendEntry {
    /// Effective local tunnel port: explicit entry port wins, otherwise the
    /// server backend default (mirrors the CLI fallback constant).
    pub fn effective_local_port(&self, default_port: u16) -> u16 {
        self.local_port.unwrap_or(default_port)
    }

    /// Effective remote server port (loopback on the remote host).
    pub fn effective_server_port(&self) -> u16 {
        self.server_port.unwrap_or(3000)
    }

    /// Tunnel base URL probed by the server prober and reported to CLI.
    pub fn local_base_url(&self, default_port: u16) -> String {
        format!(
            "http://127.0.0.1:{}",
            self.effective_local_port(default_port)
        )
    }

    /// SSH forward spec (`local:127.0.0.1:remote`) for display parity.
    pub fn forward_spec(&self, default_port: u16) -> String {
        format!(
            "{}:127.0.0.1:{}",
            self.effective_local_port(default_port),
            self.effective_server_port()
        )
    }
}

/// Whether a value is safe to use as one filesystem path component.
pub fn is_safe_name_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
        })
}

/// Validate one registry entry: empty hostnames and illegal ports fail
/// loud instead of producing an `ssh` call to nowhere.
pub fn validate_backend_entry(name: &str, entry: &BackendEntry) -> Result<(), String> {
    if !is_safe_name_component(name) {
        return Err(format!(
            "backend '{name}' must use only letters, digits, '_', '-' and '.'"
        ));
    }
    if entry.host.trim().is_empty() {
        return Err(format!("backend '{name}' has an empty host"));
    }
    if entry.username.trim().is_empty() {
        return Err(format!("backend '{name}' has an empty username"));
    }
    for (label, port) in [
        ("port", entry.port),
        ("server_port", entry.server_port),
        ("local_port", entry.local_port),
    ] {
        if port == Some(0) {
            return Err(format!("backend '{name}' has an illegal {label}: 0"));
        }
    }
    if let Some(interval) = entry.backup_interval_secs {
        if interval == 0 {
            return Err(format!(
                "backend '{name}' has an illegal backup_interval_secs: 0"
            ));
        }
    }
    if entry.backup_retention_count == Some(0) {
        return Err(format!(
            "backend '{name}' has an illegal backup_retention_count: 0"
        ));
    }
    if entry.backup_retention_days == Some(0) {
        return Err(format!(
            "backend '{name}' has an illegal backup_retention_days: 0"
        ));
    }
    // Execution ids become path components and URL segments, so a bad id is
    // rejected at write time instead of failing every backup round.
    for execution in &entry.backup_executions {
        if !is_safe_name_component(execution) {
            return Err(format!(
                "backend '{name}' has an illegal backup execution id '{execution}': use only letters, digits, '_', '-' and '.'"
            ));
        }
    }
    if let Some(directory) = entry.backup_directory.as_deref() {
        if directory.trim().is_empty() {
            return Err(format!("backend '{name}' has an empty backup_directory"));
        }
    }
    Ok(())
}

/// Runtime reachability of one backend. Unreachable rows are data, never
/// errors, so one dead backend cannot hide the rest.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BackendStatusView {
    pub reachable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_probe_ms: Option<i64>,
}

/// Runtime tunnel supervision state. The server never auto-restarts a dead
/// forward: an abnormal exit marks the entry interrupted with its reason.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelStateView {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupted_reason: Option<String>,
}

/// One registry row plus its live status: the shape `remote status`
/// consumes when the management service is reachable.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BackendEntryView {
    pub name: String,
    pub entry: BackendEntry,
    pub url: String,
    pub status: BackendStatusView,
    pub tunnel: TunnelStateView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_backup_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_error: Option<String>,
}

/// One backup index row: when the bundle was taken, for which
/// backend/execution, and its size plus checksum.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BackupRecordView {
    pub id: String,
    pub backend: String,
    pub execution: String,
    pub created_ms: i64,
    pub size_bytes: u64,
    pub sha256: String,
    pub path: String,
}

/// Result of re-reading one backup file and comparing it with its index row.
/// The status names the failure instead of collapsing every case into one
/// boolean, because "file is gone" and "content changed" need different
/// reactions.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BackupVerificationView {
    /// `ok`, `missing_file` or `digest_mismatch`.
    pub status: String,
    pub verified: bool,
    pub id: String,
    pub path: String,
    pub recorded_bytes: u64,
    pub recorded_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_sha256: Option<String>,
}

/// Raw backup payload exported for local inspection. The bundle is returned
/// unchanged; the digest and size let a local reader verify it before use.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BackupExportView {
    pub id: String,
    pub backend: String,
    pub execution: String,
    pub backup_version: u32,
    pub sha256: String,
    pub size_bytes: u64,
    pub payload: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> BackendEntry {
        BackendEntry {
            host: "example".to_string(),
            port: None,
            username: "dev".to_string(),
            key_path: None,
            jump_host: None,
            remote_dir: None,
            server_port: None,
            local_port: None,
            backup_interval_secs: None,
            backup_retention_count: None,
            backup_retention_days: None,
            backup_executions: Vec::new(),
            backup_directory: None,
        }
    }

    #[test]
    fn empty_host_fails_validation() {
        let mut bad = entry();
        bad.host.clear();
        assert!(validate_backend_entry("broken", &bad).is_err());
    }

    #[test]
    fn unsafe_name_fails_validation() {
        assert!(validate_backend_entry("../escape", &entry()).is_err());
        assert!(is_safe_name_component("host-1"));
    }

    #[test]
    fn zero_port_fails_validation() {
        let mut bad = entry();
        bad.local_port = Some(0);
        assert!(validate_backend_entry("broken", &bad).is_err());
    }

    #[test]
    fn default_ports_apply() {
        let good = entry();
        assert_eq!(good.effective_local_port(18080), 18080);
        assert_eq!(good.effective_server_port(), 3000);
        assert_eq!(good.local_base_url(18080), "http://127.0.0.1:18080");
    }

    #[test]
    fn zero_retention_fails_validation() {
        let mut bad = entry();
        bad.backup_retention_count = Some(0);
        assert!(validate_backend_entry("broken", &bad).is_err());

        let mut bad = entry();
        bad.backup_retention_days = Some(0);
        assert!(validate_backend_entry("broken", &bad).is_err());
    }

    #[test]
    fn unsafe_backup_execution_fails_validation() {
        let mut bad = entry();
        bad.backup_executions = vec!["../escape".to_string()];
        let err = validate_backend_entry("broken", &bad).unwrap_err();
        assert!(err.contains("illegal backup execution id"), "{err}");

        let mut good = entry();
        good.backup_executions = vec!["exec-1".to_string()];
        assert!(validate_backend_entry("alpha", &good).is_ok());
    }
}
