use std::sync::Arc;

use tracing::warn;

use wf_config::file_layer::{load_user_file_layer, FileLayerConfig};
use wf_config::orchestrator::{
    default_infra_file_mapping, ConfigOrchestratorBuilder, ConfigOverrides,
};
use wf_types::config::file_checkpoint::FileCheckpointConfig;
use wf_types::config::limits::LimitsConfig;
use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::presets::PresetsConfig;
use wf_types::config::storage::StorageConfig;
use wf_types::config::tool_approval::ToolApprovalConfig;
use wf_types::llm::LlmProfile;
use wf_types::llm::LlmProviderDefinition;
use wf_types::skill::SkillConfig;

use crate::error::RuntimeResult;
use crate::logger::LogConfig;
use crate::mode::{ExecutionMode, ModeInfo};

#[derive(Debug, Clone)]
pub struct CustomResourceSource {
    pub preset: wf_resource::CustomResourcesPresetConfig,
    pub base_dir: std::path::PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct ResourceConfig {
    pub options: wf_resource::registry::RegisterOptions,
    pub custom_source: Option<CustomResourceSource>,
}

impl ResourceConfig {
    pub fn apply_custom_source(&mut self) {
        let Some(source) = self.custom_source.clone() else {
            return;
        };
        let resources = wf_resource::load_custom_resources(&source.preset, &source.base_dir);
        let level = source.preset.validation_level.unwrap_or_default();
        self.options = std::mem::take(&mut self.options).with_custom_resources(resources, level);
    }
}

/// MCP settings sources used at bootstrap. When both are provided, settings
/// are merged with the priority chain:
/// `.wf/mcp.json` > global `mcp-settings.json`.
#[derive(Debug, Clone, Default)]
pub struct McpRuntimeConfig {
    /// Global settings directory (contains `mcp-settings.json`).
    pub settings_dir: Option<std::path::PathBuf>,
    /// Project root (contains `.wf/mcp.json`).
    pub project_root: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct LlmConfig {
    pub profiles: Vec<LlmProfile>,
    pub provider_definitions: Vec<LlmProviderDefinition>,
}

/// File-layer infrastructure config sources resolved through the
/// `ConfigOrchestrator` at bootstrap. Passed separately from `RuntimeConfig`
/// (see `Runtime::bootstrap_with_source`): the file layer fills the runtime
/// only where programmatic values are absent; `SdkOptions`-style overrides
/// stay the highest priority.
#[derive(Debug, Clone, Default)]
pub struct InfraSourceConfig {
    /// Project root (contains `configs/infrastructure`, `configs/skills`, ...).
    pub project_root: Option<std::path::PathBuf>,
    /// Infrastructure preset name (defaults to the `development` preset).
    pub preset_name: Option<String>,
    /// Global settings directory (contains `mcp-settings.json`,
    /// `skill-settings.json`, `infrastructure-settings.json`).
    pub settings_dir: Option<std::path::PathBuf>,
    /// Skill collection name (skill presets index mode); `None` falls back to
    /// the legacy global/project skill settings chain.
    pub skills_collection: Option<String>,
    /// Runtime environment used to select environment-optimized defaults when
    /// an infrastructure config file is missing/unparseable. `None` keeps the
    /// orchestrator default (`Development`).
    pub runtime_env: Option<wf_config::processor::infrastructure::RuntimeEnvironment>,
    /// Programmatic overrides applied on top of the file layer.
    pub overrides: ConfigOverrides,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeConfig {
    pub storage: StorageConfig,
    pub log_config: LogConfig,
    pub mode_override: Option<ExecutionMode>,
    pub resource: ResourceConfig,
    pub skills: SkillConfig,
    pub mcp: McpRuntimeConfig,
    pub metrics: Option<MetricsConfig>,
    pub llm: LlmConfig,
    /// Shell tool configuration; when `output_event_enabled` is set, shell
    /// session/output events are bridged to the runtime `EventBus`.
    pub shell: wf_shell::config::ShellToolConfig,
    /// Global sandbox configuration (profiles + routing rules). Compiled and
    /// validated at bootstrap (fail-fast); the resulting shared runtime is
    /// exposed via [`crate::Runtime::sandbox_runtime`] and injected into every
    /// script handler. `None` uses the sandbox defaults.
    pub sandbox: Option<wf_types::script::sandbox::SandboxGlobalConfig>,
    /// Output redirection defaults (resolved from the infrastructure file
    /// layer via `bootstrap_with_source`).
    pub output: OutputConfig,
    /// Runtime presets (context compression / predefined tools / prompts).
    pub presets: PresetsConfig,
    /// Tool-specific configuration sections (read_file / glob / list_files
    /// and raw pass-through sections).
    pub tools: wf_config::orchestrator::ToolConfigs,
    /// File checkpoint configuration.
    pub file_checkpoint: FileCheckpointConfig,
    /// Host default tool approval configuration. The type-level default is
    /// disabled (library contract: auto-approve); hosts enable it in their
    /// infrastructure config as a product decision.
    pub tool_approval: ToolApprovalConfig,
    /// Resource limits (agent/workflow) resolved from the infrastructure
    /// file layer via `bootstrap_with_source`; defaults otherwise.
    pub limits: LimitsConfig,
    #[cfg(feature = "plugins")]
    pub plugins: PluginConfig,
}

#[cfg(feature = "plugins")]
#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub enabled: bool,
    pub paths: Vec<std::path::PathBuf>,
    pub auto_activate: bool,
    pub guard_timeout_ms: u64,
    pub lua_enabled: bool,
    pub native_enabled: bool,
    pub wasm_enabled: bool,
}

#[cfg(feature = "plugins")]
impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            paths: vec![std::path::PathBuf::from("./plugins")],
            auto_activate: true,
            guard_timeout_ms: 10000,
            lua_enabled: true,
            native_enabled: false,
            wasm_enabled: true,
        }
    }
}

/// Apply the user file layer to fields the caller has not set explicitly:
/// file values only fill in defaults, so programmatic parameters (already
/// present in `config` when this runs) always win. Loading itself lives in
/// `wf_config::file_layer`; this only maps the loaded values onto runtime
/// fields (log level needs the runtime-owned `LogConfig`).
///
/// Precedence across the whole resolution (highest first): programmatic
/// `RuntimeConfig` > `WF_*` environment > user file layer (`~/.wf` +
/// project `.wf/config.toml`) > preset files (`configs/infrastructure`) >
/// built-in defaults. The environment check below exists because the file
/// layer runs before the orchestrator bakes `WF_*` overrides into the
/// assembled config: without it a dotfile `storage` would silently beat an
/// explicit `WF_STORAGE_*` export.
fn apply_file_layer(config: &mut RuntimeConfig, layer: &FileLayerConfig) {
    if let Some(storage) = &layer.storage {
        let storage_env_set = std::env::var_os("WF_STORAGE_TYPE").is_some()
            || std::env::var_os("WF_STORAGE_SQLITE_DB_PATH").is_some();
        if !storage_env_set && config.storage == StorageConfig::default() {
            config.storage = storage.clone();
        }
    }
    if let Some(level) = &layer.log_level {
        if config.log_config == LogConfig::default() {
            config.log_config.level = level.clone();
        }
    }
    if let Some(approval) = &layer.tool_approval {
        if config.tool_approval == wf_types::config::tool_approval::ToolApprovalConfig::default() {
            config.tool_approval = approval.clone();
        }
    }
}

pub async fn resolve_infra_config(
    mut config: RuntimeConfig,
    infra: &InfraSourceConfig,
    config_metrics: Option<&Arc<wf_metrics::ConfigMetricsCollector>>,
) -> RuntimeResult<RuntimeConfig> {
    let project_root = infra.project_root.clone().unwrap_or_default();

    // User/project config-file layer fills unset fields first, so the
    // infrastructure preset below and any caller-supplied values
    // (CLI parameters) keep their higher priority. `WF_*` environment
    // overrides beat the file layer (see `apply_file_layer`).
    let file_layer = load_user_file_layer(&project_root);
    apply_file_layer(&mut config, &file_layer);

    let preset_name = infra
        .preset_name
        .clone()
        .unwrap_or_else(|| wf_config::orchestrator::DEFAULT_INFRA_PRESET.to_string());

    let mut builder = ConfigOrchestratorBuilder::new(&project_root)
        .preset_name(Some(&preset_name))
        .default_paths(Some(default_infra_file_mapping()))
        .runtime_env(
            infra
                .runtime_env
                .unwrap_or(wf_config::processor::infrastructure::RuntimeEnvironment::Development),
        );
    if let Some(metrics) = config_metrics {
        builder = builder.with_config_metrics(metrics.clone());
    }
    let assembled = builder
        .build()
        .assemble(Some(infra.overrides.clone()))
        .map_err(|e| {
            crate::error::RuntimeError::Config(format!(
                "Infrastructure config resolution failed (preset `{preset_name}`): {e}"
            ))
        })?;

    if config.storage == StorageConfig::default() {
        config.storage = assembled.storage;
    }
    if config.output == wf_types::config::output::OutputConfig::default() {
        config.output = assembled.output;
    }
    if config.metrics.is_none() {
        config.metrics = Some(assembled.metrics);
    }
    if config.sandbox.is_none() {
        config.sandbox = assembled.sandbox;
    }
    if config.presets == wf_types::config::presets::PresetsConfig::default() {
        config.presets = assembled.presets;
    }
    if config.tools == wf_config::orchestrator::ToolConfigs::default() {
        config.tools = assembled.tools;
    }
    if config.file_checkpoint == FileCheckpointConfig::default() {
        config.file_checkpoint = assembled.file_checkpoint;
    }
    if config.tool_approval == wf_types::config::tool_approval::ToolApprovalConfig::default() {
        config.tool_approval = assembled.tool_approval;
    }
    if config.limits == wf_types::config::limits::LimitsConfig::default() {
        config.limits = assembled.limits;
    }

    // Skill settings chain (global -> project, or collection mode). Lenient:
    // a missing/invalid skill config falls back to the defaults. Without a
    // global settings dir the chain runs project-only.
    if config.skills == wf_types::skill::SkillConfig::default() {
        let skills = match infra.settings_dir.as_deref() {
            Some(settings_dir) => match &infra.skills_collection {
                Some(name) => wf_config::skill::load_and_merge_skill_config_with_collection(
                    settings_dir,
                    &project_root,
                    Some(name),
                ),
                None => wf_config::skill::load_and_merge_skill_config(settings_dir, &project_root),
            },
            None => {
                // No global settings dir configured: load only the project
                // layer instead of passing an empty sentinel path.
                wf_config::skill::load_skill_config(&wf_config::skill::get_project_skill_path(
                    &project_root,
                ))
                .map(|project| wf_config::skill::merge_skill_configs(None, project.as_ref()))
            }
        };
        match skills {
            Ok(skills) => config.skills = skills,
            Err(e) => warn!(error = %e, "failed to load skill settings chain; keeping defaults"),
        }
    }

    // MCP settings chain sources are inherited when not set explicitly.
    if config.mcp.settings_dir.is_none() {
        config.mcp.settings_dir = infra.settings_dir.clone();
    }
    if config.mcp.project_root.is_none() {
        config.mcp.project_root = infra.project_root.clone();
    }

    Ok(config)
}

pub fn adjust_log_config(mut config: LogConfig, mode_info: &ModeInfo) -> LogConfig {
    if mode_info.is_json_mode() && matches!(config.format, crate::logger::LogFormat::Full) {
        config.format = crate::logger::LogFormat::Json;
    }

    if mode_info.is_silent_mode() {
        config.level = "off".to_string();
    }

    config
}
