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
/// Default cadence of the full remote re-check in seconds (six hours).
const DEFAULT_BACKUP_RECONCILE_INTERVAL_SECS: u64 = 21_600;
/// Default install root for a deployed remote server, relative to the
/// remote login home.
const DEFAULT_DEPLOY_INSTALL_ROOT: &str = ".wf-server";
/// Default package base URL. Matches the placeholder in
/// `deploy/scripts/fetch-package.sh`; real installations override it.
const DEFAULT_DEPLOY_ARTIFACT_BASE_URL: &str = "https://packages.example.com/wf-agent";
/// Default local directory holding deploy sources (`server/`, `scripts/`).
const DEFAULT_DEPLOY_SOURCE_ROOT: &str = "deploy";
/// Default bound on one deployment command.
const DEFAULT_DEPLOY_EXEC_TIMEOUT_SECS: u64 = 120;
/// Default wait for a started remote server to answer its health endpoint.
const DEFAULT_DEPLOY_START_TIMEOUT_SECS: u64 = 30;
/// Grace period before supervised SSH forwards are killed at shutdown, giving
/// in-flight backup fetches a window to finish. Zero kills immediately.
const DEFAULT_TUNNEL_STOP_GRACE_SECS: u64 = 2;
/// Upper bound for the tunnel grace period (a typo must not stall shutdown).
const MAX_TUNNEL_STOP_GRACE_SECS: u64 = 30;

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
    /// Cadence of the full remote re-check. Zero keeps the startup pass only.
    pub backup_reconcile_interval_secs: u64,
    /// Install root for a deployed remote server, relative to the remote
    /// home. Deployment itself has no separate enable switch: enabling the
    /// backend domain is the switch, and the verb set is fixed in code.
    pub deploy_install_root: String,
    /// Base URL used by the remote host to download release artifacts.
    pub deploy_artifact_base_url: String,
    /// Local root of deploy sources, relative to the working directory when
    /// not absolute.
    pub deploy_source_root: std::path::PathBuf,
    pub deploy_exec_timeout_secs: u64,
    pub deploy_start_timeout_secs: u64,
    /// Seconds to let supervised tunnels drain before they are killed on
    /// shutdown. Bounded by `MAX_TUNNEL_STOP_GRACE_SECS`.
    pub tunnel_stop_grace_secs: u64,
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
            backup_reconcile_interval_secs: DEFAULT_BACKUP_RECONCILE_INTERVAL_SECS,
            deploy_install_root: DEFAULT_DEPLOY_INSTALL_ROOT.to_string(),
            deploy_artifact_base_url: DEFAULT_DEPLOY_ARTIFACT_BASE_URL.to_string(),
            deploy_source_root: std::path::PathBuf::from(DEFAULT_DEPLOY_SOURCE_ROOT),
            deploy_exec_timeout_secs: DEFAULT_DEPLOY_EXEC_TIMEOUT_SECS,
            deploy_start_timeout_secs: DEFAULT_DEPLOY_START_TIMEOUT_SECS,
            tunnel_stop_grace_secs: DEFAULT_TUNNEL_STOP_GRACE_SECS,
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
        if let Some(interval) = file.backup_reconcile_interval_secs {
            self.backup_reconcile_interval_secs = interval;
        }
        if let Some(root) = file.deploy_install_root {
            self.deploy_install_root = if root.trim().is_empty() {
                DEFAULT_DEPLOY_INSTALL_ROOT.to_string()
            } else {
                root.trim().to_string()
            };
        }
        if let Some(base) = file.deploy_artifact_base_url {
            self.deploy_artifact_base_url = if base.trim().is_empty() {
                String::new()
            } else {
                base.trim().trim_end_matches('/').to_string()
            };
        }
        if let Some(root) = file.deploy_source_root {
            self.deploy_source_root = if root.trim().is_empty() {
                std::path::PathBuf::from(DEFAULT_DEPLOY_SOURCE_ROOT)
            } else {
                std::path::PathBuf::from(root.trim())
            };
        }
        if let Some(timeout) = file.deploy_exec_timeout_secs {
            self.deploy_exec_timeout_secs = timeout;
        }
        if let Some(timeout) = file.deploy_start_timeout_secs {
            self.deploy_start_timeout_secs = timeout;
        }
        if let Some(grace) = file.tunnel_stop_grace_secs {
            self.tunnel_stop_grace_secs = grace;
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
        // Round cadence, not the per-target period: the loop only re-plans
        // when this interval elapses, so a test or a busy host needs it short.
        if let Some(interval) = read_env_u64("WF_BACKEND_BACKUP_INTERVAL_SECS") {
            self.backup_interval_secs = interval;
        }
        if let Some(max_bytes) = read_env_u64("WF_BACKEND_BACKUP_MAX_RESPONSE_BYTES") {
            self.backup_max_response_bytes = max_bytes;
        }
        if let Some(interval) = read_env_u64("WF_BACKEND_BACKUP_RECONCILE_INTERVAL_SECS") {
            self.backup_reconcile_interval_secs = interval;
        }
        if let Ok(raw) = std::env::var("WF_BACKEND_DEPLOY_INSTALL_ROOT") {
            self.deploy_install_root = if raw.trim().is_empty() {
                DEFAULT_DEPLOY_INSTALL_ROOT.to_string()
            } else {
                raw.trim().to_string()
            };
        }
        if let Ok(raw) = std::env::var("WF_BACKEND_DEPLOY_ARTIFACT_BASE_URL") {
            self.deploy_artifact_base_url = raw.trim().trim_end_matches('/').to_string();
        }
        if let Ok(raw) = std::env::var("WF_BACKEND_DEPLOY_SOURCE_ROOT") {
            self.deploy_source_root = if raw.trim().is_empty() {
                std::path::PathBuf::from(DEFAULT_DEPLOY_SOURCE_ROOT)
            } else {
                std::path::PathBuf::from(raw.trim())
            };
        }
        if let Some(timeout) = read_env_u64("WF_BACKEND_DEPLOY_EXEC_TIMEOUT_SECS") {
            self.deploy_exec_timeout_secs = timeout;
        }
        if let Some(timeout) = read_env_u64("WF_BACKEND_DEPLOY_START_TIMEOUT_SECS") {
            self.deploy_start_timeout_secs = timeout;
        }
        if let Some(grace) = read_env_u64("WF_BACKEND_TUNNEL_STOP_GRACE_SECS") {
            self.tunnel_stop_grace_secs = grace;
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
        if self.deploy_exec_timeout_secs == 0 {
            return Err(ConfigError::Parse(
                "invalid backend config: deploy_exec_timeout_secs must be > 0".to_string(),
            ));
        }
        if self.deploy_start_timeout_secs == 0 {
            return Err(ConfigError::Parse(
                "invalid backend config: deploy_start_timeout_secs must be > 0".to_string(),
            ));
        }
        if self.tunnel_stop_grace_secs > MAX_TUNNEL_STOP_GRACE_SECS {
            return Err(ConfigError::Parse(format!(
                "invalid backend config: tunnel_stop_grace_secs must be 0-{MAX_TUNNEL_STOP_GRACE_SECS}"
            )));
        }
        if self.deploy_install_root.trim().is_empty() {
            return Err(ConfigError::Parse(
                "invalid backend config: deploy_install_root must not be empty".to_string(),
            ));
        }
        // An empty artifact base URL is allowed: local / remote_path / url
        // sources remain usable. An empty source root is not, because scope
        // resolution would then read from the process directory by accident.
        if self.deploy_source_root.as_os_str().is_empty() {
            return Err(ConfigError::Parse(
                "invalid backend config: deploy_source_root must not be empty".to_string(),
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
    backup_reconcile_interval_secs: Option<u64>,
    deploy_install_root: Option<String>,
    deploy_artifact_base_url: Option<String>,
    deploy_source_root: Option<String>,
    deploy_exec_timeout_secs: Option<u64>,
    deploy_start_timeout_secs: Option<u64>,
    tunnel_stop_grace_secs: Option<u64>,
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

impl BackendConfig {
    /// Deployment knobs in the shape the deploy module consumes.
    #[cfg(feature = "backends")]
    pub fn deploy_settings(&self) -> crate::backends::deploy::DeploySettings {
        crate::backends::deploy::DeploySettings {
            install_root: self.deploy_install_root.clone(),
            artifact_base_url: self.deploy_artifact_base_url.clone(),
            source_root: self.deploy_source_root.clone(),
            exec_timeout_secs: self.deploy_exec_timeout_secs,
            start_timeout_secs: self.deploy_start_timeout_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_defaults_are_local_user_scoped() {
        let config = BackendConfig::default();
        assert_eq!(config.deploy_install_root, ".wf-server");
        assert_eq!(
            config.deploy_artifact_base_url,
            "https://packages.example.com/wf-agent"
        );
        assert_eq!(config.deploy_source_root, std::path::PathBuf::from("deploy"));
        assert_eq!(config.deploy_exec_timeout_secs, 120);
        assert_eq!(config.deploy_start_timeout_secs, 30);
        assert_eq!(config.tunnel_stop_grace_secs, 2);
    }

    #[test]
    fn tunnel_stop_grace_resolves_from_file_and_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let server_dir = dir.path().join("server");
        std::fs::create_dir_all(&server_dir).unwrap();
        std::fs::write(
            server_dir.join(BACKEND_CONFIG_FILE),
            "enabled = true\ntunnel_stop_grace_secs = 5\n",
        )
        .unwrap();
        let config = BackendConfig::resolve(Some(server_dir.as_path()), None).unwrap();
        assert_eq!(config.tunnel_stop_grace_secs, 5);

        std::fs::write(
            server_dir.join(BACKEND_CONFIG_FILE),
            "enabled = true\ntunnel_stop_grace_secs = 31\n",
        )
        .unwrap();
        assert!(BackendConfig::resolve(Some(server_dir.as_path()), None).is_err());
    }

    #[test]
    fn deploy_settings_resolve_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let server_dir = dir.path().join("server");
        std::fs::create_dir_all(&server_dir).unwrap();
        std::fs::write(
            server_dir.join(BACKEND_CONFIG_FILE),
            "enabled = true\ndeploy_install_root = \"wf-deploy\"\n\
             deploy_artifact_base_url = \"https://packages.example.test/builds/\"\n\
             deploy_source_root = \"fixtures/deploy\"\n\
             deploy_exec_timeout_secs = 30\n",
        )
        .unwrap();
        let config = BackendConfig::resolve(Some(server_dir.as_path()), None).unwrap();
        assert_eq!(config.deploy_install_root, "wf-deploy");
        assert_eq!(
            config.deploy_artifact_base_url,
            "https://packages.example.test/builds"
        );
        assert_eq!(
            config.deploy_source_root,
            std::path::PathBuf::from("fixtures/deploy")
        );
        assert_eq!(config.deploy_exec_timeout_secs, 30);
    }

    #[test]
    fn zero_deploy_timeout_fails_validation() {
        let config = BackendConfig {
            enabled: true,
            deploy_exec_timeout_secs: 0,
            ..BackendConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn empty_deploy_source_root_fails_validation() {
        let config = BackendConfig {
            enabled: true,
            deploy_source_root: std::path::PathBuf::new(),
            ..BackendConfig::default()
        };
        assert!(config.validate().is_err());
    }

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
