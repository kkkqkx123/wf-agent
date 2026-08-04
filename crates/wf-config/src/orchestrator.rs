//! Configuration assembly orchestrator.
//!
//! Provides a single entry point for loading infrastructure configs from
//! project/global directories, applying env var overrides, and producing
//! a fully assembled configuration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::env::{
    apply_env_overrides, env_parse_bool, env_parse_int, EnvMappingBuilder, EnvValue,
};
use crate::error::ConfigResult;
use crate::layered;
use crate::processor::infrastructure::{
    merge_metrics_with_defaults, merge_output_with_defaults, merge_storage_with_defaults,
    merge_timeout_with_defaults,
};

use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::storage::{StorageConfig, StorageType};
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

/// Declarative env var mapping for infrastructure config overrides.
fn build_infra_env_mapping() -> HashMap<String, crate::env::EnvMappingEntry> {
    EnvMappingBuilder::new()
        .custom(
            "storage_type",
            "WF_STORAGE_TYPE",
            Box::new(|v| {
                let lower = v.to_lowercase();
                Ok(EnvValue::String(lower))
            }),
            None,
        )
        .string(
            "storage_sqlite_db_path",
            "WF_STORAGE_SQLITE_DB_PATH",
            None,
        )
        .custom(
            "timeout_default",
            "WF_TIMEOUT_DEFAULT",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "metrics_enabled",
            "WF_METRICS_ENABLED",
            Box::new(env_parse_bool),
            None,
        )
        .string("output_dir", "WF_OUTPUT_DIR", None)
        .build()
}

/// Assemble configuration from project directory.
///
/// Loads `configs/infrastructure/*.toml` files, applies env var overrides,
/// then applies any programmatic overrides.
///
/// Use [`ConfigOrchestratorBuilder`] for non-default infrastructure directory.
pub struct ConfigOrchestrator;

impl ConfigOrchestrator {
    /// Assemble configuration using default paths.
    pub fn assemble(project_dir: &Path, overrides: Option<ConfigOverrides>) -> ConfigResult<AssembledConfig> {
        ConfigOrchestratorBuilder::new(project_dir)
            .build()
            .assemble(overrides)
    }
}

/// Builder for [`ConfigOrchestrator`] with non-default paths.
pub struct ConfigOrchestratorBuilder {
    infra_dir: PathBuf,
}

impl ConfigOrchestratorBuilder {
    pub fn new(project_dir: &Path) -> Self {
        Self {
            infra_dir: project_dir.join("configs").join("infrastructure"),
        }
    }

    /// Override the infrastructure config directory.
    pub fn infra_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.infra_dir = dir.into();
        self
    }

    pub fn build(self) -> ConfigOrchestratorLoaded {
        ConfigOrchestratorLoaded {
            infra_dir: self.infra_dir,
        }
    }
}

/// Orchestrator with resolved paths, ready to assemble.
pub struct ConfigOrchestratorLoaded {
    infra_dir: PathBuf,
}

impl ConfigOrchestratorLoaded {
    pub fn assemble(self, overrides: Option<ConfigOverrides>) -> ConfigResult<AssembledConfig> {
        let mut config = Self::load_infrastructure_configs(&self.infra_dir)?;
        Self::apply_env_overrides(&mut config)?;
        if let Some(o) = overrides {
            Self::apply_overrides(&mut config, o);
        }
        Ok(config)
    }

    fn load_infrastructure_configs(infra_dir: &Path) -> ConfigResult<AssembledConfig> {
        let storage_path = infra_dir.join("storage.toml");
        let timeout_path = infra_dir.join("timeout.toml");
        let metrics_path = infra_dir.join("metrics.toml");
        let output_path = infra_dir.join("output.toml");
        let sandbox_path = infra_dir.join("sandbox.toml");

        let storage: StorageConfig = if storage_path.exists() {
            layered::load_layered_config_sync(&[storage_path.as_path()]).unwrap_or_default()
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

        let storage = merge_storage_with_defaults(&storage);
        let timeout = merge_timeout_with_defaults(&timeout);
        let metrics = merge_metrics_with_defaults(&metrics);
        let output = merge_output_with_defaults(&output);

        Ok(AssembledConfig {
            storage,
            timeout,
            metrics,
            output,
            sandbox,
        })
    }

    /// Apply environment variable overrides using declarative mapping.
    fn apply_env_overrides(config: &mut AssembledConfig) -> ConfigResult<()> {
        let mapping = build_infra_env_mapping();
        apply_env_overrides(
            |key, value| match key {
                "storage_type" => {
                    if let Some(s) = value.as_string() {
                        match s {
                            "sqlite" => {
                                config.storage.storage_type = StorageType::Sqlite;
                            }
                            "postgres" => {
                                config.storage.storage_type = StorageType::Postgres;
                            }
                            "memory" => {
                                config.storage.storage_type = StorageType::Memory;
                            }
                            _ => {}
                        }
                    }
                }
                "storage_sqlite_db_path" => {
                    if let Some(path) = value.as_string() {
                        if !path.is_empty() {
                            if let Some(ref mut sqlite) = config.storage.sqlite {
                                sqlite.db_path = path.to_string();
                            } else {
                                config.storage.sqlite =
                                    Some(wf_types::config::storage::SqliteStorageConfig {
                                        db_path: path.to_string(),
                                        ..Default::default()
                                    });
                            }
                        }
                    }
                }
                "timeout_default" => {
                    if let Some(ms) = value.as_int() {
                        config.timeout.default = Some(ms);
                    }
                }
                "metrics_enabled" => {
                    if let Some(enabled) = value.as_bool() {
                        config.metrics.enabled = Some(enabled);
                    }
                }
                "output_dir" => {
                    if let Some(dir) = value.as_string() {
                        if !dir.is_empty() {
                            config.output.dir = dir.to_string();
                        }
                    }
                }
                _ => {}
            },
            &mapping,
        )
    }

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
            config.sandbox = Some(sandbox);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

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

    fn clear_wf_env_vars() {
        for var in [
            "WF_STORAGE_TYPE",
            "WF_STORAGE_SQLITE_DB_PATH",
            "WF_TIMEOUT_DEFAULT",
            "WF_METRICS_ENABLED",
            "WF_OUTPUT_DIR",
        ] {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn test_assemble_from_project_dir() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_test_project(&dir);

        let config = ConfigOrchestrator::assemble(&dir, None).unwrap();

        assert_eq!(config.storage.storage_type, StorageType::Sqlite);
        let sqlite = config.storage.sqlite.as_ref().unwrap();
        assert_eq!(sqlite.db_path, "./test.db");
        assert_eq!(config.timeout.default, Some(60000));
        assert_eq!(config.metrics.reporting_interval, Some(5000));
        assert_eq!(config.output.dir, "./test-outputs");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_defaults_when_no_files() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let config = ConfigOrchestrator::assemble(&dir, None).unwrap();

        assert_eq!(config.storage.storage_type, StorageType::Memory);
        assert_eq!(config.timeout.default, Some(30000));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_with_overrides() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

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
        assert_eq!(config.storage.storage_type, StorageType::Sqlite);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_env_override() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_test_project(&dir);

        std::env::set_var("WF_STORAGE_TYPE", "memory");
        let config = ConfigOrchestrator::assemble(&dir, None).unwrap();
        assert_eq!(config.storage.storage_type, StorageType::Memory);
        clear_wf_env_vars();

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_builder_custom_infra_dir() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-builder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let custom_infra = dir.join("custom").join("infra");
        std::fs::create_dir_all(&custom_infra).unwrap();

        let mut f = std::fs::File::create(custom_infra.join("storage.toml")).unwrap();
        writeln!(f, "type = \"postgres\"\n[postgres]\nhost = \"localhost\"\nport = 5432\nusername = \"user\"\npassword = \"pass\"\ndatabase = \"test\"").unwrap();

        let config = ConfigOrchestratorBuilder::new(&dir)
            .infra_dir(&custom_infra)
            .build()
            .assemble(None)
            .unwrap();

        assert_eq!(config.storage.storage_type, StorageType::Postgres);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
