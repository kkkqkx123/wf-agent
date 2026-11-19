//! Backend domain configuration: optional server-owned registry,
//! probing and backup behavior.
//!
//! Disabled by default; the existing direct-link chain keeps working with
//! zero behavior change. A present but invalid file fails startup instead
//! of silently falling back, so a typo cannot hide a closed backend surface.

use serde::Deserialize;
use wf_config::error::{ConfigError, ConfigResult};

/// File name of the backend domain config inside each server layer
/// directory (`backends.toml`).
pub const BACKEND_CONFIG_FILE: &str = "backends.toml";

/// Default local tunnel entry port when a registry entry leaves it unset.
/// Re-exported from `wf-types` so server-probed URLs match CLI-printed ones
/// by construction; the effective value stays overridable per server via
/// `default_local_port`.
pub use wf_types::script::schema::DEFAULT_LOCAL_TUNNEL_PORT;

/// Default probe cadence in seconds.
const DEFAULT_PROBE_INTERVAL_SECS: u64 = 30;
/// Default per-entry probe timeout in seconds.
const DEFAULT_PROBE_TIMEOUT_SECS: u64 = 5;
/// Default scheduled backup cadence in seconds (hourly).
const DEFAULT_BACKUP_INTERVAL_SECS: u64 = 3600;
/// Default backup retention: keep 10 recent bundles.
const DEFAULT_BACKUP_RETENTION_COUNT: usize = 10;
/// Default backup retention window in days (7 days).
const DEFAULT_BACKUP_RETENTION_DAYS: u64 = 7;
/// Default timeout for one remote bundle fetch.
const DEFAULT_BACKUP_TIMEOUT_SECS: u64 = 15;
/// Default upper bound on one decoded sync response, in bytes (64 MiB).
const DEFAULT_BACKUP_MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

/// Server-owned backend configuration. `enabled == false` keeps every new
/// backend endpoint at 503 and spawns no background tasks.
///
/// Enabling grants API key holders the power to make this server open
/// outbound SSH connections to registered hosts, so only enable it on a
/// trusted management server.
#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub enabled: bool,
    pub probe_interval_secs: u64,
    pub probe_timeout_secs: u64,
    pub default_local_port: u16,
    pub backup_dir: Option<std::path::PathBuf>,
    pub backup_directory: Option<String>,
    pub remote_api_key: Option<String>,
    pub registry_path: Option<std::path::PathBuf>,
    pub backup_interval_secs: u64,
    pub backup_retention_count: usize,
    pub backup_retention_days: u64,
    pub backup_timeout_secs: u64,
    pub backup_max_response_bytes: u64,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            probe_interval_secs: DEFAULT_PROBE_INTERVAL_SECS,
            probe_timeout_secs: DEFAULT_PROBE_TIMEOUT_SECS,
            default_local_port: DEFAULT_LOCAL_TUNNEL_PORT,
            backup_dir: None,
            backup_directory: None,
            remote_api_key: None,
            registry_path: None,
            backup_interval_secs: DEFAULT_BACKUP_INTERVAL_SECS,
            backup_retention_count: DEFAULT_BACKUP_RETENTION_COUNT,
            backup_retention_days: DEFAULT_BACKUP_RETENTION_DAYS,
            backup_timeout_secs: DEFAULT_BACKUP_TIMEOUT_SECS,
            backup_max_response_bytes: DEFAULT_BACKUP_MAX_RESPONSE_BYTES,
        }
    }
}

impl BackendConfig {
    /// Resolve from the server file layer (settings low, project high).
    /// Missing files fall back to defaults (disabled); a present but
    /// invalid file fails startup.
    pub fn resolve(
        settings_layer: Option<&std::path::Path>,
        project_layer: Option<&std::path::Path>,
    ) -> ConfigResult<Self> {
        let file = load_merged_backend_file(settings_layer, project_layer)?;
        let mut config = Self::default();
        if let Some(file) = file {
            config.apply_file(file)?;
        }
        config.apply_env();
        config.validate()?;
        Ok(config)
    }

    fn apply_file(&mut self, file: BackendFileConfig) -> ConfigResult<()> {
        if let Some(enabled) = file.enabled {
            self.enabled = enabled;
        }
        if let Some(interval) = file.probe_interval_secs {
            self.probe_interval_secs = interval;
        }
        if let Some(timeout) = file.probe_timeout_secs {
            self.probe_timeout_secs = timeout;
        }
        if let Some(port) = file.default_local_port {
            self.default_local_port = port;
        }
        if let Some(dir) = file.backup_dir {
            if dir.trim().is_empty() {
                self.backup_dir = None;
            } else {
                self.backup_dir = Some(std::path::PathBuf::from(dir));
            }
        }
        if let Some(directory) = file.backup_directory {
            self.backup_directory = Some(directory).filter(|value| !value.trim().is_empty());
        }
        if let Some(path) = file.registry_path {
            if path.trim().is_empty() {
                self.registry_path = None;
            } else {
                self.registry_path = Some(std::path::PathBuf::from(path));
            }
        }
        if let Some(interval) = file.backup_interval_secs {
            self.backup_interval_secs = interval;
        }
        if let Some(count) = file.backup_retention_count {
            self.backup_retention_count = count;
        }
        if let Some(days) = file.backup_retention_days {
            self.backup_retention_days = days;
        }
        if let Some(timeout) = file.backup_timeout_secs {
            self.backup_timeout_secs = timeout;
        }
        if let Some(max_bytes) = file.backup_max_response_bytes {
            self.backup_max_response_bytes = max_bytes;
        }
        Ok(())
    }

    fn apply_env(&mut self) {
        if let Some(enabled) = read_env_bool("WF_BACKEND_ENABLED") {
            self.enabled = enabled;
        }
        if let Some(interval) = read_env_u64("WF_BACKEND_PROBE_INTERVAL_SECS") {
            self.probe_interval_secs = interval;
        }
        if let Some(timeout) = read_env_u64("WF_BACKEND_PROBE_TIMEOUT_SECS") {
            self.probe_timeout_secs = timeout;
        }
        if let Some(port) = read_env_u64("WF_BACKEND_DEFAULT_LOCAL_PORT") {
            if let Ok(port) = u16::try_from(port) {
                self.default_local_port = port;
            }
        }
        if let Ok(raw) = std::env::var("WF_BACKEND_BACKUP_DIR") {
            if raw.trim().is_empty() {
                self.backup_dir = None;
            } else {
                self.backup_dir = Some(std::path::PathBuf::from(raw));
            }
        }
        if let Ok(raw) = std::env::var("WF_BACKEND_BACKUP_DIRECTORY") {
            self.backup_directory = Some(raw).filter(|value| !value.trim().is_empty());
        }
        if let Ok(raw) = std::env::var("WF_BACKEND_REMOTE_API_KEY") {
            self.remote_api_key = Some(raw).filter(|value| !value.trim().is_empty());
        }
        if let Ok(raw) = std::env::var("WF_BACKEND_REGISTRY_PATH") {
            if raw.trim().is_empty() {
                self.registry_path = None;
            } else {
                self.registry_path = Some(std::path::PathBuf::from(raw));
            }
        }
        if let Some(timeout) = read_env_u64("WF_BACKEND_BACKUP_TIMEOUT_SECS") {
            self.backup_timeout_secs = timeout;
        }
        if let Some(max_bytes) = read_env_u64("WF_BACKEND_BACKUP_MAX_RESPONSE_BYTES") {
            self.backup_max_response_bytes = max_bytes;
        }
    }

    fn validate(&self) -> ConfigResult<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.probe_interval_secs == 0 {
            return Err(ConfigError::Parse(
                "invalid backend config: probe_interval_secs must be > 0".to_string(),
            ));
        }
        if self.probe_timeout_secs == 0 {
            return Err(ConfigError::Parse(
                "invalid backend config: probe_timeout_secs must be > 0".to_string(),
            ));
        }
        if self.probe_timeout_secs > self.probe_interval_secs {
            return Err(ConfigError::Parse(
                "invalid backend config: probe_timeout_secs must not exceed probe_interval_secs"
                    .to_string(),
            ));
        }
        if self.default_local_port == 0 {
            return Err(ConfigError::Parse(
                "invalid backend config: default_local_port must be 1-65535".to_string(),
            ));
        }
        if self.backup_interval_secs == 0 {
            return Err(ConfigError::Parse(
                "invalid backend config: backup_interval_secs must be > 0".to_string(),
            ));
        }
        // A zero cap would delete every backup of a target, including the one
        // just written, so both caps must keep at least one.
        if self.backup_retention_count == 0 {
            return Err(ConfigError::Parse(
                "invalid backend config: backup_retention_count must be > 0".to_string(),
            ));
        }
        if self.backup_retention_days == 0 {
            return Err(ConfigError::Parse(
                "invalid backend config: backup_retention_days must be > 0".to_string(),
            ));
        }
        if self.backup_timeout_secs == 0 {
            return Err(ConfigError::Parse(
                "invalid backend config: backup_timeout_secs must be > 0".to_string(),
            ));
        }
        if self.backup_max_response_bytes == 0 {
            return Err(ConfigError::Parse(
                "invalid backend config: backup_max_response_bytes must be > 0".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct BackendFileConfig {
    enabled: Option<bool>,
    probe_interval_secs: Option<u64>,
    probe_timeout_secs: Option<u64>,
    default_local_port: Option<u16>,
    backup_dir: Option<String>,
    backup_directory: Option<String>,
    registry_path: Option<String>,
    backup_interval_secs: Option<u64>,
    backup_retention_count: Option<usize>,
    backup_retention_days: Option<u64>,
    backup_timeout_secs: Option<u64>,
    backup_max_response_bytes: Option<u64>,
}

fn load_merged_backend_file(
    settings_layer: Option<&std::path::Path>,
    project_layer: Option<&std::path::Path>,
) -> ConfigResult<Option<BackendFileConfig>> {
    let mut paths = Vec::new();
    if let Some(dir) = settings_layer {
        paths.push(dir.join(BACKEND_CONFIG_FILE));
    }
    if let Some(dir) = project_layer {
        paths.push(dir.join(BACKEND_CONFIG_FILE));
    }
    let existing: Vec<&std::path::Path> = paths
        .iter()
        .map(|path| path.as_path())
        .filter(|path| path.exists())
        .collect();
    if existing.is_empty() {
        return Ok(None);
    }
    wf_config::layered::load_layered_config_sync::<BackendFileConfig>(&existing)
        .map(Some)
        .map_err(|e| {
            ConfigError::Parse(format!(
                "invalid backend config files {}: {e}",
                existing
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

fn read_env_bool(name: &str) -> Option<bool> {
    let raw = std::env::var(name).ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    match wf_config::env::env_parse_bool(&raw) {
        Ok(wf_config::env::EnvValue::Bool(v)) => Some(v),
        Ok(_) => None,
        Err(e) => {
            tracing::warn!(error = %e, var = name, "invalid env bool; keeping current value");
            None
        }
    }
}

fn read_env_u64(name: &str) -> Option<u64> {
    let raw = std::env::var(name).ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    match wf_config::env::env_parse_int(raw.trim()) {
        Ok(wf_config::env::EnvValue::Int(v)) => match u64::try_from(v) {
            Ok(v) => Some(v),
            Err(_) => {
                tracing::warn!(var = name, value = %raw, "negative env int; keeping current value");
                None
            }
        },
        Ok(_) => None,
        Err(e) => {
            tracing::warn!(error = %e, var = name, "invalid env int; keeping current value");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_by_default() {
        let config = BackendConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn invalid_probe_window_fails_when_enabled() {
        let config = BackendConfig {
            enabled: true,
            probe_interval_secs: 5,
            probe_timeout_secs: 10,
            ..BackendConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn disabled_skips_validation() {
        let config = BackendConfig {
            probe_interval_secs: 0,
            ..BackendConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn invalid_file_fails_startup() {
        let dir = tempfile::tempdir().unwrap();
        let server_dir = dir.path().join("server");
        std::fs::create_dir_all(&server_dir).unwrap();
        std::fs::write(
            server_dir.join(BACKEND_CONFIG_FILE),
            "enabled = true\nprobe_interval_secs = 0\n",
        )
        .unwrap();
        let result = BackendConfig::resolve(Some(server_dir.as_path()), None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("probe_interval_secs"));
    }

    #[test]
    fn backup_directory_resolves_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let server_dir = dir.path().join("server");
        std::fs::create_dir_all(&server_dir).unwrap();
        std::fs::write(
            server_dir.join(BACKEND_CONFIG_FILE),
            "enabled = true\nbackup_directory = \"/workspace/alpha\"\n",
        )
        .unwrap();
        let config = BackendConfig::resolve(Some(server_dir.as_path()), None).unwrap();
        assert_eq!(config.backup_directory.as_deref(), Some("/workspace/alpha"));
        assert_eq!(config.remote_api_key, None);
    }

    #[test]
    fn registry_path_resolves_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let server_dir = dir.path().join("server");
        std::fs::create_dir_all(&server_dir).unwrap();
        std::fs::write(
            server_dir.join(BACKEND_CONFIG_FILE),
            "enabled = true\nregistry_path = \"/tmp/wf-registry.db\"\n",
        )
        .unwrap();
        let config = BackendConfig::resolve(Some(server_dir.as_path()), None).unwrap();
        assert_eq!(
            config.registry_path,
            Some(std::path::PathBuf::from("/tmp/wf-registry.db"))
        );
    }
}
