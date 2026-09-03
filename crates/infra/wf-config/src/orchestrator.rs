//! Configuration assembly orchestrator.
//!
//! Provides a single entry point for loading infrastructure configs from
//! project/global directories, applying env var overrides, and producing
//! a fully assembled configuration.
//!
//! Infrastructure presets: when
//! `configs/infrastructure/index.json` exists and a preset name is given,
//! the preset's `files` mapping drives which file backs each config domain.
//! Without an index/preset, fixed default filenames are used (legacy mode).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::env::apply_env_overrides;
use crate::error::{ConfigError, ConfigResult};
use crate::layered;
use crate::orchestrator_loader::{load_domain_config, normalize_camel_case, resolve_file_mapping};
use crate::processor::file_checkpoint::merge_file_checkpoint_with_defaults;
use crate::processor::infrastructure::{
    get_metrics_environment_defaults, get_output_environment_defaults,
    get_storage_environment_defaults, get_timeout_environment_defaults, merge_metrics_with_defaults,
    merge_output_with_defaults, merge_storage_with_defaults, merge_timeout_with_defaults,
    RuntimeEnvironment,
};
use crate::processor::limits::{merge_limits_with_defaults, validate_limits_config};
use crate::processor::presets::{
    get_presets_environment_defaults, transform_presets_config,
};
use crate::processor::sandbox_global::validate_sandbox_global;
use crate::processor::tools::{
    transform_glob_config, transform_list_files_config, transform_read_file_config, GlobConfig,
    GlobConfigInput, ListFilesConfig, ListFilesConfigInput, ReadFileConfig, ReadFileConfigInput,
};

use wf_types::config::file_checkpoint::FileCheckpointConfig;
use wf_types::config::limits::LimitsConfig;
use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::presets::PresetsConfig;
use wf_types::config::storage::{StorageConfig, StorageType};
use wf_types::config::timeout::TimeoutConfig;
use wf_types::config::tool_approval::ToolApprovalConfig;
use wf_types::script::sandbox::SandboxGlobalConfig;

/// Default infrastructure preset used when no explicit preset is provided.
pub const DEFAULT_INFRA_PRESET: &str = "development";

/// File names backing each infrastructure config domain.
#[derive(Debug, Clone, Default)]
pub struct InfrastructurePresetFiles {
    pub storage: String,
    pub timeout: String,
    pub metrics: String,
    pub output: String,
    pub sandbox: String,
    pub file_checkpoint: String,
    pub tool_approval: String,
    pub presets: String,
    pub tools: String,
    pub limits: String,
}

impl InfrastructurePresetFiles {
    /// Fixed default filenames (legacy mode, used when no preset is resolved).
    pub fn default_filenames() -> Self {
        Self {
            storage: "storage.toml".to_string(),
            timeout: "timeout.toml".to_string(),
            metrics: "metrics.toml".to_string(),
            output: "output.toml".to_string(),
            sandbox: "sandbox.toml".to_string(),
            file_checkpoint: "file-checkpoint.toml".to_string(),
            tool_approval: "tool-approval.toml".to_string(),
            presets: "presets.toml".to_string(),
            tools: "tools.toml".to_string(),
            limits: "limits.toml".to_string(),
        }
    }
}

/// Default infrastructure file mapping (legacy filenames).
pub fn default_infra_file_mapping() -> InfrastructurePresetFiles {
    InfrastructurePresetFiles::default_filenames()
}

/// Tool-specific configuration sections (read_file / glob / list_files and
/// raw pass-through sections).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolConfigs {
    pub glob: Option<GlobConfig>,
    pub list_files: Option<ListFilesConfig>,
    pub read_file: Option<ReadFileConfig>,
    /// Sections without a dedicated processor, passed through verbatim
    /// (e.g. `writeFile`, `editFile`, `runShell`, ...).
    pub passthrough: HashMap<String, serde_json::Value>,
}

/// Fully assembled configuration produced by the orchestrator.
#[derive(Debug, Clone)]
pub struct AssembledConfig {
    pub storage: StorageConfig,
    pub timeout: TimeoutConfig,
    pub metrics: MetricsConfig,
    pub output: OutputConfig,
    pub sandbox: Option<SandboxGlobalConfig>,
    pub file_checkpoint: FileCheckpointConfig,
    pub tool_approval: wf_types::config::tool_approval::ToolApprovalConfig,
    pub presets: PresetsConfig,
    pub tools: ToolConfigs,
    pub limits: LimitsConfig,
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
            file_checkpoint: FileCheckpointConfig::default(),
            tool_approval: wf_types::config::tool_approval::ToolApprovalConfig::default(),
            presets: PresetsConfig::default(),
            tools: ToolConfigs::default(),
            limits: LimitsConfig::default(),
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
    pub file_checkpoint: Option<FileCheckpointConfig>,
    pub tool_approval: Option<wf_types::config::tool_approval::ToolApprovalConfig>,
    pub presets: Option<PresetsConfig>,
    pub tools: Option<ToolConfigs>,
    pub limits: Option<LimitsConfig>,
}

pub struct ConfigOrchestrator;

impl ConfigOrchestrator {
    /// Assemble configuration using default paths.
    pub fn assemble(
        project_dir: &Path,
        overrides: Option<ConfigOverrides>,
    ) -> ConfigResult<AssembledConfig> {
        ConfigOrchestratorBuilder::new(project_dir)
            .build()
            .assemble(overrides)
    }

    /// Assemble configuration with an infrastructure preset (TS
    /// `loadInfrastructureConfigs`). When `preset_name` is given and
    /// `configs/infrastructure/index.json` exists, the preset's `files`
    /// mapping selects the config files per domain; otherwise `default_paths`
    /// (or the fixed default filenames) are used.
    pub fn assemble_with_preset(
        project_dir: &Path,
        preset_name: Option<&str>,
        default_paths: Option<InfrastructurePresetFiles>,
        overrides: Option<ConfigOverrides>,
    ) -> ConfigResult<AssembledConfig> {
        ConfigOrchestratorBuilder::new(project_dir)
            .preset_name(preset_name)
            .default_paths(default_paths)
            .build()
            .assemble(overrides)
    }
}

/// Builder for [`ConfigOrchestrator`] with non-default paths.
pub struct ConfigOrchestratorBuilder {
    infra_dir: PathBuf,
    preset_name: Option<String>,
    default_paths: Option<InfrastructurePresetFiles>,
    runtime_env: RuntimeEnvironment,
}

impl ConfigOrchestratorBuilder {
    pub fn new(project_dir: &Path) -> Self {
        Self {
            infra_dir: project_dir.join("configs").join("infrastructure"),
            preset_name: None,
            default_paths: None,
            runtime_env: RuntimeEnvironment::Development,
        }
    }

    /// Override the infrastructure config directory.
    pub fn infra_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.infra_dir = dir.into();
        self
    }

    /// Set the infrastructure preset name (e.g. "development").
    pub fn preset_name(mut self, name: Option<&str>) -> Self {
        self.preset_name = name.map(|s| s.to_string());
        self
    }

    /// Set the fallback file mapping used when no preset is resolved.
    pub fn default_paths(mut self, paths: Option<InfrastructurePresetFiles>) -> Self {
        self.default_paths = paths;
        self
    }

    /// Set the runtime environment used to select environment-optimized
    /// defaults when a config file is missing or unparseable.
    pub fn runtime_env(mut self, env: RuntimeEnvironment) -> Self {
        self.runtime_env = env;
        self
    }

    pub fn build(self) -> ConfigOrchestratorLoaded {
        ConfigOrchestratorLoaded {
            infra_dir: self.infra_dir,
            preset_name: self.preset_name,
            default_paths: self.default_paths,
            runtime_env: self.runtime_env,
        }
    }
}

/// Orchestrator with resolved paths, ready to assemble.
pub struct ConfigOrchestratorLoaded {
    infra_dir: PathBuf,
    preset_name: Option<String>,
    default_paths: Option<InfrastructurePresetFiles>,
    runtime_env: RuntimeEnvironment,
}

impl ConfigOrchestratorLoaded {
    pub fn assemble(self, overrides: Option<ConfigOverrides>) -> ConfigResult<AssembledConfig> {
        let files = resolve_file_mapping(
            &self.infra_dir,
            self.preset_name.as_deref(),
            self.default_paths,
        );
        let mut config = Self::load_infrastructure_configs(&self.infra_dir, &files, self.runtime_env)?;
        Self::apply_env_overrides(&mut config)?;
        if let Some(o) = overrides {
            Self::apply_overrides(&mut config, o)?;
        }
        Ok(config)
    }

    /// Load all infrastructure config domains using the resolved file mapping.
    /// Missing/unparseable files fall back to `runtime_env`-optimized defaults.
    fn load_infrastructure_configs(
        infra_dir: &Path,
        files: &InfrastructurePresetFiles,
        runtime_env: RuntimeEnvironment,
    ) -> ConfigResult<AssembledConfig> {
        let storage_path = infra_dir.join(&files.storage);
        let timeout_path = infra_dir.join(&files.timeout);
        let metrics_path = infra_dir.join(&files.metrics);
        let output_path = infra_dir.join(&files.output);
        let sandbox_path = infra_dir.join(&files.sandbox);
        let limits_path = infra_dir.join(&files.limits);

        let storage: StorageConfig =
            load_domain_config(&storage_path, get_storage_environment_defaults(runtime_env));
        let timeout: TimeoutConfig =
            load_domain_config(&timeout_path, get_timeout_environment_defaults(runtime_env));
        let metrics: MetricsConfig =
            load_domain_config(&metrics_path, get_metrics_environment_defaults(runtime_env));
        let output: OutputConfig =
            load_domain_config(&output_path, get_output_environment_defaults(runtime_env));
        let limits: LimitsConfig = load_domain_config(&limits_path, LimitsConfig::default());

        // Sandbox config is fail-fast: a malformed sandbox.toml must reject
        // startup instead of silently running with the weaker defaults.
        let sandbox: Option<SandboxGlobalConfig> = if sandbox_path.exists() {
            let config: SandboxGlobalConfig =
                layered::load_layered_config_sync(&[sandbox_path.as_path()]).map_err(|e| {
                    ConfigError::Validation(format!(
                        "Invalid sandbox global config in {}: {e}",
                        sandbox_path.display()
                    ))
                })?;
            validate_sandbox_global(&config)?;
            Some(config)
        } else {
            None
        };

        // File-checkpoint / tool-approval / presets / tools load leniently:
        // parse failures fall back to defaults (presets additionally fail
        // fast on validation errors).
        let file_checkpoint = load_domain_config::<FileCheckpointConfig>(
            &infra_dir.join(&files.file_checkpoint),
            FileCheckpointConfig::default(),
        );
        let tool_approval = load_domain_config::<ToolApprovalConfig>(
            &infra_dir.join(&files.tool_approval),
            ToolApprovalConfig::default(),
        );
        let presets = transform_presets_config(load_domain_config::<PresetsConfig>(
            &infra_dir.join(&files.presets),
            get_presets_environment_defaults(runtime_env),
        ))?;
        let tools = load_tool_configs(infra_dir, files);

        let storage = merge_storage_with_defaults(&storage);
        let timeout = merge_timeout_with_defaults(&timeout);
        let metrics = merge_metrics_with_defaults(&metrics);
        let output = merge_output_with_defaults(&output);
        let file_checkpoint = merge_file_checkpoint_with_defaults(&file_checkpoint);
        let limits = merge_limits_with_defaults(&limits);
        validate_limits_config(&limits)?;

        Ok(AssembledConfig {
            storage,
            timeout,
            metrics,
            output,
            sandbox,
            file_checkpoint,
            tool_approval,
            presets,
            tools,
            limits,
        })
    }

    /// Apply environment variable overrides using declarative mapping.
    fn apply_env_overrides(config: &mut AssembledConfig) -> ConfigResult<()> {
        let mapping = crate::orchestrator_env::build_infra_env_mapping();
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
                "limits_agent_max_iterations_cap" => {
                    if let Some(v) = value.as_int() {
                        set_agent_limit(&mut config.limits, |l| {
                            l.max_iterations_cap = Some(v as u32)
                        });
                    }
                }
                "limits_agent_default_max_iterations" => {
                    if let Some(v) = value.as_int() {
                        set_agent_limit(&mut config.limits, |l| {
                            l.default_max_iterations = Some(v as u32)
                        });
                    }
                }
                "limits_agent_max_concurrent" => {
                    if let Some(v) = value.as_int() {
                        set_agent_limit(&mut config.limits, |l| l.max_concurrent = Some(v as u32));
                    }
                }
                "limits_agent_max_sub_agent_depth" => {
                    if let Some(v) = value.as_int() {
                        set_agent_limit(&mut config.limits, |l| {
                            l.max_sub_agent_depth = Some(v as u32)
                        });
                    }
                }
                "limits_agent_max_pause_duration_ms" => {
                    if let Some(v) = value.as_int() {
                        set_agent_limit(&mut config.limits, |l| {
                            l.max_pause_duration_ms = Some(v as u64)
                        });
                    }
                }
                "limits_workflow_loop_max_iterations_cap" => {
                    if let Some(v) = value.as_int() {
                        set_workflow_limit(&mut config.limits, |l| {
                            l.loop_max_iterations_cap = Some(v as u32)
                        });
                    }
                }
                "limits_workflow_loop_default_max_iterations" => {
                    if let Some(v) = value.as_int() {
                        set_workflow_limit(&mut config.limits, |l| {
                            l.loop_default_max_iterations = Some(v as u32)
                        });
                    }
                }
                "limits_workflow_max_navigation_multiplier" => {
                    if let Some(v) = value.as_int() {
                        set_workflow_limit(&mut config.limits, |l| {
                            l.max_navigation_multiplier = Some(v as u32)
                        });
                    }
                }
                "limits_exec_node_timeout_ms" => {
                    if let Some(v) = value.as_int() {
                        set_exec_default(&mut config.limits, |l| {
                            l.node_timeout_ms = Some(v as u64)
                        });
                    }
                }
                "limits_exec_max_execution_time_ms" => {
                    if let Some(v) = value.as_int() {
                        set_exec_default(&mut config.limits, |l| {
                            l.max_execution_time_ms = Some(v as u64)
                        });
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

    fn apply_overrides(
        config: &mut AssembledConfig,
        overrides: ConfigOverrides,
    ) -> ConfigResult<()> {
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
        if let Some(file_checkpoint) = overrides.file_checkpoint {
            config.file_checkpoint = merge_file_checkpoint_with_defaults(&file_checkpoint);
        }
        if let Some(tool_approval) = overrides.tool_approval {
            config.tool_approval = tool_approval;
        }
        if let Some(presets) = overrides.presets {
            config.presets = transform_presets_config(presets)?;
        }
        if let Some(tools) = overrides.tools {
            config.tools = tools;
        }
        if let Some(limits) = overrides.limits {
            config.limits = merge_limits_with_defaults(&limits);
            validate_limits_config(&config.limits)?;
        }
        Ok(())
    }
}

/// Update the `agent` section of the limits config, materializing it when
/// absent.
fn set_agent_limit(
    config: &mut LimitsConfig,
    update: impl FnOnce(&mut wf_types::config::limits::AgentLimits),
) {
    let entry = config
        .agent
        .get_or_insert_with(wf_types::config::limits::AgentLimits::default);
    update(entry);
}

/// Update the `workflow` section of the limits config, materializing it
/// when absent.
fn set_workflow_limit(
    config: &mut LimitsConfig,
    update: impl FnOnce(&mut wf_types::config::limits::WorkflowLimits),
) {
    let entry = config
        .workflow
        .get_or_insert_with(wf_types::config::limits::WorkflowLimits::default);
    update(entry);
}

/// Update the `execution_defaults` section of the limits config,
/// materializing it when absent.
fn set_exec_default(
    config: &mut LimitsConfig,
    update: impl FnOnce(&mut wf_types::config::limits::ExecutionDefaults),
) {
    let entry = config
        .execution_defaults
        .get_or_insert_with(wf_types::config::limits::ExecutionDefaults::default);
    update(entry);
}

/// Load a single-file preset definition (JSON) and extract its `files`
/// mapping, resolving relative paths against the preset file's directory.
pub fn load_tool_configs(infra_dir: &Path, files: &InfrastructurePresetFiles) -> ToolConfigs {
    let path = infra_dir.join(&files.tools);
    let mut tools = ToolConfigs::default();
    if !path.exists() {
        return tools;
    }
    let value: toml::Value = match layered::load_layered_config_sync(&[path.as_path()]) {
        Ok(value) => value,
        Err(e) => {
            warn!(
                error = %e,
                "failed to parse tool configs {}; falling back to defaults",
                path.display()
            );
            return tools;
        }
    };
    let Some(table) = value.as_table() else {
        return tools;
    };
    for (section, section_value) in table {
        let json = serde_json::to_value(section_value).unwrap_or(serde_json::Value::Null);
        match section.as_str() {
            "read_file" | "readFile" => {
                let input = normalize_camel_case(json);
                match serde_json::from_value::<ReadFileConfigInput>(input)
                    .map_err(ConfigError::from)
                    .and_then(transform_read_file_config)
                {
                    Ok(config) => tools.read_file = Some(config),
                    Err(e) => {
                        warn!(error = %e, "invalid [read_file] section in {}", path.display())
                    }
                }
            }
            "glob" => {
                let input = normalize_camel_case(json);
                match serde_json::from_value::<GlobConfigInput>(input)
                    .map_err(ConfigError::from)
                    .and_then(transform_glob_config)
                {
                    Ok(config) => tools.glob = Some(config),
                    Err(e) => warn!(error = %e, "invalid [glob] section in {}", path.display()),
                }
            }
            "list_files" | "listFiles" => {
                let input = normalize_camel_case(json);
                match serde_json::from_value::<ListFilesConfigInput>(input)
                    .map_err(ConfigError::from)
                    .and_then(transform_list_files_config)
                {
                    Ok(config) => tools.list_files = Some(config),
                    Err(e) => {
                        warn!(error = %e, "invalid [list_files] section in {}", path.display())
                    }
                }
            }
            _ => {
                tools.passthrough.insert(section.clone(), json);
            }
        }
    }
    tools
}

/// Load infrastructure configs using the default preset
/// (`development`), with the fixed default filenames as fallback.
pub fn load_default_infrastructure_configs(project_dir: &Path) -> ConfigResult<AssembledConfig> {
    ConfigOrchestrator::assemble_with_preset(
        project_dir,
        Some(DEFAULT_INFRA_PRESET),
        Some(default_infra_file_mapping()),
        None,
    )
}

#[cfg(test)]
#[path = "orchestrator_test.rs"]
mod orchestrator_test;
