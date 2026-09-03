use wf_config::orchestrator::ConfigOverrides;
use wf_types::config::file_checkpoint::FileCheckpointConfig;
use wf_types::config::limits::LimitsConfig;
use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::presets::PresetsConfig;
use wf_types::config::storage::StorageConfig;
use wf_types::config::timeout::TimeoutConfig;
use wf_types::config::tool_approval::ToolApprovalConfig;
use wf_types::llm::LlmProfile;
use wf_types::skill::SkillConfig;

use crate::logger::LogConfig;
use crate::mode::ExecutionMode;

#[derive(Debug, Clone, Default)]
pub struct ResourceConfig {
    pub options: wf_resource::registry::RegisterOptions,
}

/// MCP settings sources used at bootstrap. When both are provided, settings
/// are merged with the priority chain:
/// `.wf/mcp.json` > `.agent/mcp.json` > global `mcp-settings.json`.
#[derive(Debug, Clone, Default)]
pub struct McpRuntimeConfig {
    /// Global settings directory (contains `mcp-settings.json`).
    pub settings_dir: Option<std::path::PathBuf>,
    /// Project root (contains `.wf/mcp.json` / `.agent/mcp.json`).
    pub project_root: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct LlmConfig {
    pub profiles: Vec<LlmProfile>,
}

/// File-layer infrastructure config sources resolved through the
/// `ConfigOrchestrator` at bootstrap. The file layer fills the runtime only
/// where programmatic values are absent; `SdkOptions`-style overrides stay
/// the highest priority.
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
    /// Execution timeout defaults (resolved from the infrastructure file
    /// layer when `infra` is set).
    pub timeout: TimeoutConfig,
    /// Output redirection defaults (resolved from the infrastructure file
    /// layer when `infra` is set).
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
    /// File-layer infrastructure config source; `None` keeps the runtime
    /// programmatic-only (storage/metrics/sandbox defaults).
    pub infra: Option<InfraSourceConfig>,
    /// Resource limits (agent/workflow) resolved from the infrastructure
    /// file layer when `infra` is set; defaults otherwise.
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
}

#[cfg(feature = "plugins")]
impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            paths: vec![std::path::PathBuf::from("./plugins")],
            auto_activate: true,
            guard_timeout_ms: 10000,
        }
    }
}
