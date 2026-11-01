use std::sync::Arc;

use tracing::warn;

use wf_config::file_layer::{load_user_file_layer, FileLayerConfig};
use wf_config::orchestrator::{ConfigOrchestratorBuilder, ConfigOverrides};
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

/// Opt-in online catalog refresh (revision: disabled by default, no persistence).
///
/// When `enabled` is false (the default, including `None` on `LlmConfig`),
/// bootstrap keeps the current behavior: static LiteLLM table only. When
/// true, bootstrap may call `ModelCatalog::refresh_filtered` in the
/// background; failures degrade to the static table with a warning and
/// explicit profile fields always win (`explicit > network > static`).
/// No `wf-storage` persistence in the first cut; results stay in memory.
/// TLS/timeout tuning reuses `LlmHttpTransport`, so no `allow_insecure` lives here.
#[derive(Debug, Clone, Default)]
pub struct CatalogRefreshConfig {
    /// Master switch. `false` (default) keeps static-table-only behavior.
    pub enabled: bool,
    /// Provider ids to refresh; `None` means all registered definitions.
    pub provider_ids: Option<Vec<String>>,
    /// Per-provider refresh timeout in seconds; `None` uses the client default.
    pub timeout_secs: Option<u64>,
}

impl CatalogRefreshConfig {
    /// Whether an online refresh should run. `false` keeps zero behavior change.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug, Clone, Default)]
pub struct LlmConfig {
    pub profiles: Vec<LlmProfile>,
    pub provider_definitions: Vec<LlmProviderDefinition>,
    /// Declarative authentication: credential store location plus the
    /// provider auth surfaces. Empty keeps the static-key behavior.
    pub auth: wf_types::config::llm_auth::AuthConfig,
    /// Opt-in online catalog refresh. `None` (default) disables it.
    pub catalog_refresh: Option<CatalogRefreshConfig>,
}

/// File-layer infrastructure config sources resolved through the
/// `ConfigOrchestrator` at bootstrap. Passed separately from `RuntimeConfig`
/// (see `Runtime::bootstrap_with_source`): the file layer fills the runtime
/// only where programmatic values are absent; `SdkOptions`-style overrides
/// stay the highest priority.
#[derive(Debug, Clone, Default)]
pub struct InfraSourceConfig {
    /// Project root carrying the `.wf/` project layer. Shared deployment
    /// defaults are not read from here; production values arrive through
    /// the settings layer, the project layer, or an explicit managed
    /// directory (see `deploy/README.md`).
    pub project_root: Option<std::path::PathBuf>,
    /// Global settings directory (contains `mcp-settings.json`,
    /// `skill-settings.json`).
    pub settings_dir: Option<std::path::PathBuf>,
    /// Opt into ambient workstation state (XDG config home, `~/.wf`, the
    /// in-repository demo directory). Workstation CLIs enable this;
    /// services keep the default (`false`) so process environment can
    /// never inject configuration and tenants stay isolated.
    pub allow_ambient: bool,
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
    /// File checkpoint configuration. `None` means the caller did not set it
    /// explicitly, so the assembled infrastructure value fills it; `Some`
    /// carries an explicit programmatic value (including `enabled = false`,
    /// which must not be mistaken for "unset") and wins per the merge rules.
    pub file_checkpoint: Option<FileCheckpointConfig>,
    /// User repository Git configuration. `None` leaves the domain unset and
    /// the assembled infrastructure value fills it; `Some` is explicit.
    pub user_git: Option<wf_types::config::user_git::UserGitConfig>,
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
            paths: wf_plugin::engine::config::default_plugin_scan_paths(),
            auto_activate: true,
            guard_timeout_ms: 10000,
            lua_enabled: true,
            native_enabled: false,
            wasm_enabled: true,
        }
    }
}

/// Map `tools.toml [shell]` onto `ShellToolConfig`: file values only fill
/// programmatic defaults (programmatic wins). Precedence downstream stays
/// `ShellToolConfig > sandbox profile > built-in` via `CommandPolicy`.
fn apply_shell_tool_config(
    shell: &mut wf_shell::config::ShellToolConfig,
    file: Option<&wf_config::processor::tools::ShellToolConfig>,
) {
    let Some(file) = file else { return };
    if let Some(dir) = file.workspace_dir.as_deref() {
        if shell.workspace_dir.is_none() {
            shell.workspace_dir = Some(std::path::PathBuf::from(dir));
        }
    }
    if let Some(timeout) = file.max_timeout_ms {
        if shell.max_timeout_ms == wf_shell::config::DEFAULT_MAX_TIMEOUT_MS {
            shell.max_timeout_ms = timeout;
        }
    }
    if let Some(allowed) = file.allowed_commands.clone() {
        if shell.allowed_commands
            == wf_shell::config::DEFAULT_ALLOWED_COMMANDS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        {
            shell.allowed_commands = allowed;
        }
    }
    if shell.denied_commands.is_none() {
        shell.denied_commands = file.denied_commands.clone();
    }
    if shell.read_commands.is_none() {
        shell.read_commands = file.read_commands.clone();
    }
    if shell.write_commands.is_none() {
        shell.write_commands = file.write_commands.clone();
    }
}

/// Apply the user file layer to fields the caller has not set explicitly:
/// file values only fill in defaults, so programmatic parameters (already
/// present in `config` when this runs) always win. Loading itself lives in
/// `wf_config::file_layer`; this only maps the loaded values onto runtime
/// fields (log level needs the runtime-owned `LogConfig`).
///
/// Precedence across the whole resolution (highest first): programmatic
/// `RuntimeConfig` (including `ConfigOverrides`) > `WF_*` environment >
/// explicit managed directory (ops-rendered, e.g. from `deploy/`) > project
/// `.wf/` layer (including the host-knob `config.toml`) > explicit settings
/// dir (ambient workstation state joins only when the caller opts in) >
/// built-in code defaults. The environment check below exists because the
/// file layer runs before the orchestrator bakes `WF_*` overrides into the
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

    // Volatile `.wf` subdirectories (meta/tmp with internal ignores) are
    // ensured before any layer reads the project: only files inside
    // `.wf/{meta,tmp}` are created, the outer workspace `.gitignore` is
    // never touched. Best effort: a failure warns and bootstrap continues.
    if !project_root.as_os_str().is_empty() {
        if let Err(e) = wf_config::layout::ensure_project_wf_volatile_dirs(&project_root) {
            warn!(error = %e, "failed to ensure volatile .wf directories; continuing");
        }
    }

    // User/project config-file layer fills unset fields first, so the
    // fixed infrastructure files below and any caller-supplied values
    // (CLI parameters) keep their higher priority. `WF_*` environment
    // overrides beat the file layer (see `apply_file_layer`). The global
    // side is the explicit settings dir; ambient workstation state only
    // joins when the caller opts in, so services never read the daemon
    // home directory by default.
    let file_global = infra.settings_dir.clone().or_else(|| {
        if infra.allow_ambient {
            wf_config::file_layer::user_config_dir()
        } else {
            None
        }
    });
    let file_layer = load_user_file_layer(file_global, &project_root);
    apply_file_layer(&mut config, &file_layer);

    let mut builder = ConfigOrchestratorBuilder::new(&project_root).runtime_env(
        infra
            .runtime_env
            .unwrap_or(wf_config::processor::infrastructure::RuntimeEnvironment::Development),
    );
    if let Some(settings_dir) = &infra.settings_dir {
        builder = builder.settings_dir(settings_dir.clone());
    }
    if let Some(metrics) = config_metrics {
        builder = builder.with_config_metrics(metrics.clone());
    }
    let assembled = builder
        .build()
        .assemble(Some(infra.overrides.clone()))
        .map_err(|e| {
            crate::error::RuntimeError::Config(format!(
                "Infrastructure config resolution failed: {e}"
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
    // Programmatic overrides already won inside the assembled config, so
    // they outrank the programmatic runtime values below. Explicitness is
    // carried by `Option`: `None` means the caller left file checkpoint
    // unset and the assembled value fills it; `Some` survives verbatim so
    // an explicitly closed file channel (`enabled = false`) cannot be
    // mistaken for "unset" and covered by the assembled value. An explicit
    // infra override still wins over an explicit programmatic value.
    config.file_checkpoint = Some(
        match (
            config.file_checkpoint,
            infra.overrides.file_checkpoint.as_ref(),
        ) {
            (_, Some(_)) => assembled.file_checkpoint,
            (Some(explicit), None) => explicit,
            (None, None) => assembled.file_checkpoint,
        },
    );
    if infra.overrides.sandbox.is_some() || config.sandbox.is_none() {
        config.sandbox = assembled.sandbox;
    }
    if config.presets == wf_types::config::presets::PresetsConfig::default() {
        config.presets = assembled.presets;
    }
    if config.tools == wf_config::orchestrator::ToolConfigs::default() {
        config.tools = assembled.tools;
    }
    apply_shell_tool_config(&mut config.shell, config.tools.shell.as_ref());
    if config.tool_approval == wf_types::config::tool_approval::ToolApprovalConfig::default() {
        config.tool_approval = assembled.tool_approval;
    }
    // User git has no dedicated infrastructure domain file yet; the
    // programmatic value wins when present, otherwise a disabled default
    // keeps the domain off until the host configures a repo root.
    if config.user_git.is_none() {
        config.user_git = Some(wf_types::config::user_git::UserGitConfig::default());
    }
    if config.limits == wf_types::config::limits::LimitsConfig::default() {
        config.limits = assembled.limits;
    }

    // Skill settings chain (global -> project). Lenient:
    // a missing/invalid skill config falls back to the defaults. Without a
    // global settings dir the chain runs project-only.
    if config.skills == wf_types::skill::SkillConfig::default() {
        let skills = match infra.settings_dir.as_deref() {
            Some(settings_dir) => {
                wf_config::skill::load_and_merge_skill_config(settings_dir, &project_root)
            }
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

    // Load declarative LLM auth surfaces from the explicit project and
    // settings-dir `llm-auth` families so `LlmConfig.auth` can be driven
    // by config files instead of programmatic defaults. Ambient
    // home-directory sources only join when the caller opts in. An
    // explicitly populated auth config keeps its higher priority.
    if config.llm.auth == wf_types::config::llm_auth::AuthConfig::default()
        && !project_root.as_os_str().is_empty()
    {
        match wf_config::llm_auth::load_llm_auth_config(
            &project_root,
            infra.settings_dir.as_deref(),
            infra.allow_ambient,
        ) {
            Ok(auth) => {
                if !auth.providers.is_empty() {
                    config.llm.auth = auth;
                }
            }
            Err(e) => warn!(error = %e, "failed to load llm-auth config; keeping defaults"),
        }
    }
    // Credential store placement: with ambient disabled the store resolves
    // next to the project so tenants never share the daemon home
    // directory. Workstation CLIs keep the home-directory default by
    // leaving ambient enabled. An explicitly configured store path always
    // wins.
    if !infra.allow_ambient
        && config.llm.auth.store_path.is_none()
        && !project_root.as_os_str().is_empty()
    {
        config.llm.auth.store_path = Some(
            project_root
                .join(wf_config::layout::PROJECT_WF_DIR)
                .join("auth.json")
                .to_string_lossy()
                .into_owned(),
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_refresh_is_disabled_by_default() {
        assert!(LlmConfig::default().catalog_refresh.is_none());
        assert!(!CatalogRefreshConfig::default().is_enabled());
        assert!(!CatalogRefreshConfig {
            enabled: false,
            provider_ids: Some(vec!["p".to_string()]),
            timeout_secs: Some(5),
        }
        .is_enabled());
        assert!(CatalogRefreshConfig {
            enabled: true,
            ..Default::default()
        }
        .is_enabled());
    }
}
