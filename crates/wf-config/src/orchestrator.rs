//! Configuration assembly orchestrator.
//!
//! Provides a single entry point for loading infrastructure configs from
//! project/global directories, applying env var overrides, and producing
//! a fully assembled configuration.

use std::path::Path;

use crate::error::ConfigResult;
use crate::layered;
use crate::processor::infrastructure::{
    merge_metrics_with_defaults, merge_output_with_defaults,
    merge_storage_with_defaults, merge_timeout_with_defaults,
};

use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::storage::StorageConfig;
use wf_types::config::timeout::TimeoutConfig;
use wf_types::script::sandbox::SandboxGlobalConfig;

/// Fully assembled configuration produced by the orchestrator.
#[derive(Debug, Clone)]
pub struct AssembledConfig {
    pub storage: StorageConfig,
    pub timeout: TimeoutConfig,
    pub metrics: MetricsConfig,
    pub output: OutputConfig,
    pub sandbox: Option<SandboxGlobalConfig>,
}

impl Default for AssembledConfig {
    fn default() -> Self {
        Self {
            storage: StorageConfig::default(),
            timeout: TimeoutConfig::default(),
            metrics: MetricsConfig::default(),
            output: OutputConfig {
                dir: "./outputs".to_string(),
                log_file_pattern: "app-{date}.log".to_string(),
                enable_log_terminal: true,
                enable_sdk_logs: true,
                sdk_log_level: wf_types::config::output::SdkLogLevel::Warn,
            },
            sandbox: None,
        }
    }
}

/// Programmatic overrides applied after file-based config loading.
#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
    pub storage: Option<StorageConfig>,
    pub timeout: Option<TimeoutConfig>,
    pub metrics: Option<MetricsConfig>,
    pub output: Option<OutputConfig>,
    pub sandbox: Option<SandboxGlobalConfig>,
}

/// Single assembly entry for configuration.
pub struct ConfigOrchestrator;

impl ConfigOrchestrator {
    /// Assemble configuration from project directory.
    ///
    /// Loads `configs/infrastructure/*.toml` files, applies env var overrides,
    /// then applies any programmatic overrides.
    pub fn assemble(
        project_dir: &Path,
        overrides: Option<ConfigOverrides>,
    ) -> ConfigResult<AssembledConfig> {
        let infra_dir = project_dir.join("configs").join("infrastructure");

        let mut config = Self::load_infrastructure_configs(&infra_dir)?;
        Self::apply_env_overrides(&mut config);
        if let Some(o) = overrides {
            Self::apply_overrides(&mut config, o);
        }
        Ok(config)
    }

    /// Load infrastructure config files from the given directory.
    fn load_infrastructure_configs(infra_dir: &Path) -> ConfigResult<AssembledConfig> {
        let storage_path = infra_dir.join("storage.toml");
        let timeout_path = infra_dir.join("timeout.toml");
        let metrics_path = infra_dir.join("metrics.toml");
        let output_path = infra_dir.join("output.toml");
        let sandbox_path = infra_dir.join("sandbox.toml");

        let storage: StorageConfig = if storage_path.exists() {
            layered::load_layered_config_sync(&[storage_path.as_path()])
                .unwrap_or_default()
        } else {
            StorageConfig::default()
        };

        let timeout: TimeoutConfig = if timeout_path.exists() {
            layered::load_layered_config_sync(&[timeout_path.as_path()]).unwrap_or_default()
        } else {
            TimeoutConfig::default()
        };

        let metrics: MetricsConfig = if metrics_path.exists() {
            layered::load_layered_config_sync(&[metrics_path.as_path()]).unwrap_or_default()
        } else {
            MetricsConfig::default()
        };

        let output: OutputConfig = if output_path.exists() {
            layered::load_layered_config_sync(&[output_path.as_path()]).unwrap_or_default()
        } else {
            AssembledConfig::default().output
        };

        let sandbox: Option<SandboxGlobalConfig> = if sandbox_path.exists() {
            layered::load_layered_config_sync(&[sandbox_path.as_path()]).ok()
        } else {
            None
        };

        // Merge each config with defaults via processor functions.
        let storage = merge_storage_with_defaults(&storage);
        let timeout = merge_timeout_with_defaults(&timeout);
        let metrics = merge_metrics_with_defaults(&metrics);
        let output = merge_output_with_defaults(&output);
        let sandbox = sandbox.map(|s| merge_sandbox_with_defaults_sandbox(&s));

        Ok(AssembledConfig {
            storage,
            timeout,
            metrics,
            output,
            sandbox,
        })
    }

    /// Apply environment variable overrides (WF_* prefix).
    fn apply_env_overrides(config: &mut AssembledConfig) {
        // Storage type override
        if let Ok(val) = std::env::var("WF_STORAGE_TYPE") {
            match val.to_lowercase().as_str() {
                "sqlite" => config.storage.storage_type = wf_types::config::storage::StorageType::Sqlite,
                "postgres" => config.storage.storage_type = wf_types::config::storage::StorageType::Postgres,
                "memory" => config.storage.storage_type = wf_types::config::storage::StorageType::Memory,
                _ => {}
            }
        }

        // Storage db_path override
        if let Ok(val) = std::env::var("WF_STORAGE_SQLITE_DB_PATH") {
            if !val.is_empty() {
                if let Some(ref mut sqlite) = config.storage.sqlite {
                    sqlite.db_path = val;
                } else {
                    config.storage.sqlite = Some(wf_types::config::storage::SqliteStorageConfig {
                        db_path: val,
                        ..Default::default()
                    });
                }
            }
        }

        // Timeout default override
        if let Ok(val) = std::env::var("WF_TIMEOUT_DEFAULT") {
            if let Ok(ms) = val.parse::<i64>() {
                config.timeout.default = Some(ms);
            }
        }

        // Metrics enabled override
        if let Ok(val) = std::env::var("WF_METRICS_ENABLED") {
            let enabled = val.to_lowercase() == "true" || val == "1";
            config.metrics.enabled = Some(enabled);
        }

        // Output dir override
        if let Ok(val) = std::env::var("WF_OUTPUT_DIR") {
            if !val.is_empty() {
                config.output.dir = val;
            }
        }
    }

    /// Apply programmatic overrides.
    fn apply_overrides(config: &mut AssembledConfig, overrides: ConfigOverrides) {
        if let Some(storage) = overrides.storage {
            config.storage = merge_storage_with_defaults(&storage);
        }
        if let Some(timeout) = overrides.timeout {
            config.timeout = merge_timeout_with_defaults(&timeout);
        }
        if let Some(metrics) = overrides.metrics {
            config.metrics = merge_metrics_with_defaults(&metrics);
        }
        if let Some(output) = overrides.output {
            config.output = merge_output_with_defaults(&output);
        }
        if let Some(sandbox) = overrides.sandbox {
            config.sandbox = Some(merge_sandbox_with_defaults_sandbox(&sandbox));
        }
    }
}

/// Merge sandbox global config with defaults (wrapper around processor).
fn merge_sandbox_with_defaults_sandbox(config: &SandboxGlobalConfig) -> SandboxGlobalConfig {
    config.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn setup_test_project(dir: &Path) {
        let infra = dir.join("configs").join("infrastructure");
        std::fs::create_dir_all(&infra).unwrap();

        let mut f = std::fs::File::create(infra.join("storage.toml")).unwrap();
        writeln!(f, "type = \"sqlite\"\n[sqlite]\ndb_path = \"./test.db\"").unwrap();

        let mut f = std::fs::File::create(infra.join("timeout.toml")).unwrap();
        writeln!(f, "default = 60000").unwrap();

        let mut f = std::fs::File::create(infra.join("metrics.toml")).unwrap();
        writeln!(f, "enabled = true\nreporting_interval = 5000").unwrap();

        let mut f = std::fs::File::create(infra.join("output.toml")).unwrap();
        writeln!(f, "dir = \"./test-outputs\"\nlog_file_pattern = \"test.log\"\nenable_log_terminal = false\nenable_sdk_logs = false\nsdk_log_level = \"info\"").unwrap();
    }

    #[test]
    fn test_assemble_from_project_dir() {
        let dir = std::env::temp_dir().join(format!("wf-orch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_test_project(&dir);

        let config = ConfigOrchestrator::assemble(&dir, None).unwrap();

        assert_eq!(config.storage.storage_type, wf_types::config::storage::StorageType::Sqlite);
        let sqlite = config.storage.sqlite.as_ref().unwrap();
        assert_eq!(sqlite.db_path, "./test.db");
        assert_eq!(config.timeout.default, Some(60000));
        assert_eq!(config.metrics.reporting_interval, Some(5000));
        assert_eq!(config.output.dir, "./test-outputs");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_defaults_when_no_files() {
        let dir = std::env::temp_dir().join(format!("wf-orch-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let config = ConfigOrchestrator::assemble(&dir, None).unwrap();

        // Should get defaults from Default impl, then merge_with_defaults fills in
        assert_eq!(config.storage.storage_type, wf_types::config::storage::StorageType::Memory);
        // merge_timeout_with_defaults fills in the default value
        assert_eq!(config.timeout.default, Some(30000));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_with_overrides() {
        let dir = std::env::temp_dir().join(format!("wf-orch-override-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_test_project(&dir);

        let overrides = ConfigOverrides {
            timeout: Some(TimeoutConfig {
                default: Some(99999),
                ..Default::default()
            }),
            ..Default::default()
        };

        let config = ConfigOrchestrator::assemble(&dir, Some(overrides)).unwrap();
        assert_eq!(config.timeout.default, Some(99999));
        // Storage should still come from file
        assert_eq!(config.storage.storage_type, wf_types::config::storage::StorageType::Sqlite);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_env_override() {
        let dir = std::env::temp_dir().join(format!("wf-orch-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_test_project(&dir);

        std::env::set_var("WF_STORAGE_TYPE", "memory");
        let config = ConfigOrchestrator::assemble(&dir, None).unwrap();
        assert_eq!(config.storage.storage_type, wf_types::config::storage::StorageType::Memory);
        std::env::remove_var("WF_STORAGE_TYPE");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
