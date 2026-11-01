//! Configuration assembly orchestrator.
//!
//! Provides a single entry point for loading infrastructure configs from
//! the fixed domain filenames, applying env var
//! overrides, and producing a fully assembled configuration. Missing files
//! fall back to code defaults; overrides and environment win over files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::env::apply_env_overrides;
use crate::error::{ConfigError, ConfigResult};
use crate::layered;
use crate::orchestrator_loader::{display_paths, load_merged_domain_config_with_metrics};
use crate::processor::file_checkpoint::{
    merge_file_checkpoint_with_defaults, validate_file_checkpoint_config,
};
use crate::processor::infrastructure::{
    get_metrics_environment_defaults, get_output_environment_defaults,
    get_storage_environment_defaults, merge_metrics_with_defaults, merge_output_with_defaults,
    merge_storage_with_defaults, RuntimeEnvironment,
};
use crate::processor::limits::{merge_limits_with_defaults, validate_limits_config};
use crate::processor::presets::{get_presets_environment_defaults, transform_presets_config};
use crate::processor::sandbox_global::{merge_sandbox_global_config, validate_sandbox_global};
use crate::processor::tools::{
    transform_code_context_config, transform_glob_config, transform_list_files_config,
    transform_read_file_config, transform_shell_tool_config, CodeContextConfigInput, GlobConfig,
    GlobConfigInput, ListFilesConfig, ListFilesConfigInput, ReadFileConfig, ReadFileConfigInput,
};
pub use crate::processor::tools::{ShellToolConfig, ShellToolConfigInput};
use wf_integration::CodeContextConfig;
use wf_types::config::file_checkpoint::FileCheckpointConfig;
use wf_types::config::limits::LimitsConfig;
use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::presets::PresetsConfig;
use wf_types::config::storage::{StorageConfig, StorageType};
use wf_types::config::tool_approval::ToolApprovalConfig;
use wf_types::script::sandbox::{SandboxGlobalConfig, SandboxMode};

/// Fixed file names backing each infrastructure config domain.
#[derive(Debug, Clone, Default)]
pub struct InfrastructurePresetFiles {
    pub storage: String,
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
    /// Fixed domain filenames, resolved per domain from the explicit
    /// override, the settings layer and the project layer.
    pub fn default_filenames() -> Self {
        Self {
            storage: "storage.toml".to_string(),
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

/// Tool-specific configuration sections (read_file / glob / list_files /
/// shell and raw pass-through sections).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolConfigs {
    pub glob: Option<GlobConfig>,
    pub list_files: Option<ListFilesConfig>,
    pub read_file: Option<ReadFileConfig>,
    /// Host-level shell defaults (`tools.toml [shell]`). `None` inherits;
    /// explicit lists override profile and built-in read/write lists.
    pub shell: Option<ShellToolConfig>,
    /// Code-context service config backing file folding and the retrieval
    /// tools. `None` (the default) disables the service.
    pub code_context: Option<CodeContextConfig>,
    /// Sections without a dedicated processor, passed through verbatim
    /// (e.g. `writeFile`, `editFile`, `runShell`, ...).
    pub passthrough: HashMap<String, serde_json::Value>,
}

/// Fully assembled configuration produced by the orchestrator.
#[derive(Debug, Clone)]
pub struct AssembledConfig {
    pub storage: StorageConfig,
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
}

/// Builder for [`ConfigOrchestrator`] with non-default paths.
pub struct ConfigOrchestratorBuilder {
    project_dir: PathBuf,
    settings_dir: Option<PathBuf>,
    explicit_infra_dir: Option<PathBuf>,
    runtime_env: RuntimeEnvironment,
    config_metrics: Option<std::sync::Arc<wf_metrics::ConfigMetricsCollector>>,
}

impl ConfigOrchestratorBuilder {
    pub fn new(project_dir: &Path) -> Self {
        Self {
            project_dir: project_dir.to_path_buf(),
            settings_dir: None,
            explicit_infra_dir: None,
            runtime_env: RuntimeEnvironment::Development,
            config_metrics: None,
        }
    }

    /// Attach the config collector; assembly accesses, load durations and
    /// validation failures are recorded into it.
    pub fn with_config_metrics(
        mut self,
        metrics: std::sync::Arc<wf_metrics::ConfigMetricsCollector>,
    ) -> Self {
        self.config_metrics = Some(metrics);
        self
    }

    /// Override the settings layer directory (`{settings_dir}/infrastructure`).
    pub fn settings_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.settings_dir = Some(dir.into());
        self
    }

    /// Override the infrastructure config directory outright. An explicit
    /// directory replaces the settings and project layers for every domain.
    pub fn infra_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.explicit_infra_dir = Some(dir.into());
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
            project_dir: self.project_dir,
            settings_dir: self.settings_dir,
            explicit_infra_dir: self.explicit_infra_dir,
            runtime_env: self.runtime_env,
            config_metrics: self.config_metrics,
        }
    }
}

/// Orchestrator with resolved paths, ready to assemble.
pub struct ConfigOrchestratorLoaded {
    project_dir: PathBuf,
    settings_dir: Option<PathBuf>,
    explicit_infra_dir: Option<PathBuf>,
    runtime_env: RuntimeEnvironment,
    config_metrics: Option<std::sync::Arc<wf_metrics::ConfigMetricsCollector>>,
}

impl ConfigOrchestratorLoaded {
    pub fn assemble(self, overrides: Option<ConfigOverrides>) -> ConfigResult<AssembledConfig> {
        let metrics = self.config_metrics.clone();
        if let Some(ref metrics) = metrics {
            metrics.record_access();
        }
        let start = std::time::Instant::now();
        let result = self.assemble_inner(overrides);
        match &result {
            Ok(_) => {
                if let Some(ref metrics) = metrics {
                    metrics.record_load_complete(start.elapsed().as_millis() as f64);
                }
            }
            Err(_) => {
                if let Some(ref metrics) = metrics {
                    metrics.record_validation_error();
                }
            }
        }
        result
    }

    fn assemble_inner(self, overrides: Option<ConfigOverrides>) -> ConfigResult<AssembledConfig> {
        let files = default_infra_file_mapping();
        let mut config = Self::load_infrastructure_configs(
            self.explicit_infra_dir.as_deref(),
            self.settings_dir.as_deref(),
            &self.project_dir,
            &files,
            self.runtime_env,
            self.config_metrics.as_deref(),
        )?;
        Self::apply_env_overrides(&mut config)?;
        if let Some(o) = overrides {
            Self::apply_overrides(&mut config, o)?;
        }
        Ok(config)
    }

    /// Load all infrastructure config domains using the resolved file mapping.
    /// Per domain the explicit override wins when set; otherwise the
    /// settings layer and the project layer merge with the project winning.
    /// Missing files fall back to `runtime_env`-optimized defaults; present
    /// but invalid files fail.
    fn load_infrastructure_configs(
        explicit_infra_dir: Option<&Path>,
        settings_dir: Option<&Path>,
        project_dir: &Path,
        files: &InfrastructurePresetFiles,
        runtime_env: RuntimeEnvironment,
        metrics: Option<&wf_metrics::ConfigMetricsCollector>,
    ) -> ConfigResult<AssembledConfig> {
        let candidates = |file: &str| -> Vec<PathBuf> {
            match explicit_infra_dir {
                Some(dir) => vec![dir.join(file)],
                None => {
                    let mut paths = Vec::new();
                    if let Some(settings) = settings_dir {
                        paths.push(crate::layout::settings_infra_dir(settings).join(file));
                    }
                    paths.push(crate::layout::project_infra_dir(project_dir).join(file));
                    paths
                }
            }
        };

        let storage: StorageConfig = load_merged_domain_config_with_metrics(
            &candidates(&files.storage),
            get_storage_environment_defaults(runtime_env),
            metrics,
        )?;
        let metrics_config: MetricsConfig = load_merged_domain_config_with_metrics(
            &candidates(&files.metrics),
            get_metrics_environment_defaults(runtime_env),
            metrics,
        )?;
        let output: OutputConfig = load_merged_domain_config_with_metrics(
            &candidates(&files.output),
            get_output_environment_defaults(runtime_env),
            metrics,
        )?;
        let limits: LimitsConfig = load_merged_domain_config_with_metrics(
            &candidates(&files.limits),
            LimitsConfig::default(),
            metrics,
        )?;

        // Sandbox config is fail-fast: a malformed sandbox.toml must reject
        // startup instead of silently running with the weaker defaults.
        let sandbox_paths = candidates(&files.sandbox);
        let sandbox_existing: Vec<&Path> = sandbox_paths
            .iter()
            .map(PathBuf::as_path)
            .filter(|path| path.exists())
            .collect();
        let sandbox: Option<SandboxGlobalConfig> = if sandbox_existing.is_empty() {
            None
        } else {
            let config: SandboxGlobalConfig =
                layered::load_layered_config_sync_with_metrics(&sandbox_existing, metrics)
                    .map_err(|e| {
                        ConfigError::Validation(format!(
                            "Invalid sandbox global config in {}: {e}",
                            display_paths(&sandbox_existing)
                        ))
                    })?;
            validate_sandbox_global(&config)?;
            Some(config)
        };

        // Missing domain files fall back to defaults; present but invalid
        // files fail.
        let file_checkpoint = load_merged_domain_config_with_metrics::<FileCheckpointConfig>(
            &candidates(&files.file_checkpoint),
            FileCheckpointConfig::default(),
            metrics,
        )?;
        let tool_approval = load_merged_domain_config_with_metrics::<ToolApprovalConfig>(
            &candidates(&files.tool_approval),
            ToolApprovalConfig::default(),
            metrics,
        )?;
        let presets =
            transform_presets_config(load_merged_domain_config_with_metrics::<PresetsConfig>(
                &candidates(&files.presets),
                get_presets_environment_defaults(runtime_env),
                metrics,
            )?)?;
        let tools = load_tool_configs(&candidates(&files.tools))?;

        let storage = merge_storage_with_defaults(&storage);
        let metrics_config = merge_metrics_with_defaults(&metrics_config);
        let output = merge_output_with_defaults(&output);
        let file_checkpoint = merge_file_checkpoint_with_defaults(&file_checkpoint);
        let limits = merge_limits_with_defaults(&limits);
        if let Err(e) = validate_limits_config(&limits) {
            if let Some(metrics) = metrics {
                metrics.record_validation_error();
            }
            return Err(e);
        }
        if let Err(e) = validate_file_checkpoint_config(&file_checkpoint) {
            if let Some(metrics) = metrics {
                metrics.record_validation_error();
            }
            return Err(e);
        }

        Ok(AssembledConfig {
            storage,
            metrics: metrics_config,
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
                "file_checkpoint_enabled" => {
                    if let Some(enabled) = value.as_bool() {
                        config.file_checkpoint.enabled = enabled;
                    }
                }
                "file_checkpoint_workspace_root" => {
                    if let Some(root) = value.as_string() {
                        if !root.is_empty() {
                            config.file_checkpoint.workspace_root = Some(root.to_string());
                        }
                    }
                }
                "file_checkpoint_manual_watch" => {
                    if let Some(watch) = value.as_bool() {
                        config.file_checkpoint.manual_watch = watch;
                    }
                }
                "file_checkpoint_manual_poll_ms" => {
                    if let Some(v) = value.as_int() {
                        if let Ok(poll_ms) = u64::try_from(v) {
                            config.file_checkpoint.manual_poll_ms = Some(poll_ms);
                        }
                    }
                }
                "file_checkpoint_gc_interval_secs" => {
                    if let Some(v) = value.as_int() {
                        if let Ok(interval) = u64::try_from(v) {
                            config.file_checkpoint.gc_interval_secs = Some(interval);
                        }
                    }
                }
                "sandbox_mode" => {
                    if let Some(mode) = value.as_string() {
                        // Only the canonical spellings arrive here; the
                        // mapping parser rejects everything else.
                        let mode = match mode {
                            "Disabled" => Some(SandboxMode::Disabled),
                            "Lenient" => Some(SandboxMode::Lenient),
                            "Strict" => Some(SandboxMode::Strict),
                            _ => None,
                        };
                        if let Some(mode) = mode {
                            match config.sandbox.as_mut() {
                                Some(global) => global.mode = Some(mode),
                                None => {
                                    config.sandbox = Some(SandboxGlobalConfig {
                                        mode: Some(mode),
                                        ..Default::default()
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            },
            &mapping,
        )?;
        // Environment overrides participate in the same fail-fast contract
        // as file values: a zero poll interval or an out-of-bounds default
        // scope introduced here aborts startup.
        validate_file_checkpoint_config(&config.file_checkpoint)?;
        if let Some(sandbox) = config.sandbox.as_ref() {
            validate_sandbox_global(sandbox)?;
        }
        Ok(())
    }

    fn apply_overrides(
        config: &mut AssembledConfig,
        overrides: ConfigOverrides,
    ) -> ConfigResult<()> {
        if let Some(storage) = overrides.storage {
            config.storage = merge_storage_with_defaults(&storage);
        }
        if let Some(metrics) = overrides.metrics {
            config.metrics = merge_metrics_with_defaults(&metrics);
        }
        if let Some(output) = overrides.output {
            config.output = merge_output_with_defaults(&output);
        }
        if let Some(sandbox) = overrides.sandbox {
            let base = config.sandbox.clone().unwrap_or_default();
            let merged = merge_sandbox_global_config(&base, &sandbox);
            validate_sandbox_global(&merged)?;
            config.sandbox = Some(merged);
        }
        if let Some(file_checkpoint) = overrides.file_checkpoint {
            config.file_checkpoint = merge_file_checkpoint_with_defaults(&file_checkpoint);
            validate_file_checkpoint_config(&config.file_checkpoint)?;
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

/// Load tool-specific sections from candidate `tools.toml` files in
/// precedence order (low to high). Missing files yield defaults; a present
/// file with invalid sections fails. Unknown sections are kept verbatim but
/// must be tables so mistyped scalars cannot hide as opaque tool config.
pub fn load_tool_configs(paths: &[PathBuf]) -> ConfigResult<ToolConfigs> {
    let existing: Vec<&Path> = paths
        .iter()
        .map(PathBuf::as_path)
        .filter(|path| path.exists())
        .collect();
    let mut tools = ToolConfigs::default();
    if existing.is_empty() {
        return Ok(tools);
    }
    let sources = display_paths(&existing);
    let value: toml::Value = layered::load_layered_config_sync(&existing)
        .map_err(|e| ConfigError::Parse(format!("invalid tool configs {sources}: {e}")))?;
    let Some(table) = value.as_table() else {
        return Err(ConfigError::Validation(format!(
            "invalid tool configs {sources}: top-level must be a table"
        )));
    };
    for (section, section_value) in table {
        let json = serde_json::to_value(section_value).map_err(ConfigError::from)?;
        match section.as_str() {
            "read_file" => {
                let config = serde_json::from_value::<ReadFileConfigInput>(json)
                    .map_err(ConfigError::from)
                    .and_then(transform_read_file_config)
                    .map_err(|e| {
                        ConfigError::Validation(format!(
                            "invalid [read_file] section in {sources}: {e}"
                        ))
                    })?;
                tools.read_file = Some(config);
            }
            "glob" => {
                let config = serde_json::from_value::<GlobConfigInput>(json)
                    .map_err(ConfigError::from)
                    .and_then(transform_glob_config)
                    .map_err(|e| {
                        ConfigError::Validation(format!("invalid [glob] section in {sources}: {e}"))
                    })?;
                tools.glob = Some(config);
            }
            "list_files" => {
                let config = serde_json::from_value::<ListFilesConfigInput>(json)
                    .map_err(ConfigError::from)
                    .and_then(transform_list_files_config)
                    .map_err(|e| {
                        ConfigError::Validation(format!(
                            "invalid [list_files] section in {sources}: {e}"
                        ))
                    })?;
                tools.list_files = Some(config);
            }
            "code_context" => {
                let config = serde_json::from_value::<CodeContextConfigInput>(json)
                    .map_err(ConfigError::from)
                    .and_then(transform_code_context_config)
                    .map_err(|e| {
                        ConfigError::Validation(format!(
                            "invalid [code_context] section in {sources}: {e}"
                        ))
                    })?;
                tools.code_context = Some(config);
            }
            "shell" => {
                let config = serde_json::from_value::<ShellToolConfigInput>(json)
                    .map_err(ConfigError::from)
                    .and_then(transform_shell_tool_config)
                    .map_err(|e| {
                        ConfigError::Validation(format!(
                            "invalid [shell] section in {sources}: {e}"
                        ))
                    })?;
                tools.shell = Some(config);
            }
            _ => {
                if !json.is_object() {
                    return Err(ConfigError::Validation(format!(
                        "invalid [{section}] section in {sources}: passthrough tool config must be a table",
                    )));
                }
                tools.passthrough.insert(section.clone(), json);
            }
        }
    }
    Ok(tools)
}

/// Load infrastructure configs from the fixed domain filenames, falling back
/// to code defaults per domain. Reads the explicit override when the builder
/// sets one, otherwise the settings layer and the project layer.
pub fn load_default_infrastructure_configs(project_dir: &Path) -> ConfigResult<AssembledConfig> {
    ConfigOrchestrator::assemble(project_dir, None)
}

#[cfg(test)]
#[path = "orchestrator_test.rs"]
mod orchestrator_test;
