use std::sync::Arc;

use tracing::info;
use wf_workflow::trigger_listener::TriggerEventListener;

use wf_config::processor::infrastructure::merge_metrics_with_defaults;
use wf_config::processor::llm_profile::{transform_llm_profile, validate_llm_profile};
use wf_core::event::EventBus;
use wf_llm::LlmGateway;
use wf_resource::registrar::{Options as ResourceOptions, Registries};
use wf_resource::starter::BundleRegistry;
use wf_types::config::metrics::MetricsConfig;
use wf_types::llm::LlmProfile;

use crate::error::RuntimeResult;
use crate::lifecycle::{shutdown_channel, ShutdownHandle, ShutdownWaiter};
use crate::logger::{init_tracing, LogConfig};
use crate::metrics::MetricsContext;
use crate::mode::{detect_all, ModeInfo};
use crate::storage_manager::{StorageConfig, StorageManager};
use crate::trigger_listener::{
    start_trigger_listener_with_registry, ExecutionContextRegistry,
};

#[derive(Debug, Clone, Default)]
pub struct ResourceConfig {
    pub options: ResourceOptions,
}

/// MCP settings sources used at bootstrap. When both are provided, settings
/// are merged with the TS-compatible priority chain:
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

#[derive(Debug, Clone, Default)]
pub struct RuntimeConfig {
    pub storage: StorageConfig,
    pub log_config: LogConfig,
    pub mode_override: Option<super::mode::ExecutionMode>,
    pub resource: ResourceConfig,
    pub skills: wf_types::skill::SkillConfig,
    pub mcp: McpRuntimeConfig,
    pub metrics: Option<MetricsConfig>,
    pub llm: LlmConfig,
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

pub struct Runtime {
    pub storage_manager: StorageManager,
    pub mode_info: ModeInfo,
    pub shutdown_handle: ShutdownHandle,
    pub _shutdown_waiter: ShutdownWaiter,
    pub registries: Arc<Registries>,
    pub bundles: Arc<BundleRegistry>,
    pub skill_loader: Arc<wf_tools::SkillLoader>,
    /// Shared tool registry (builtin handlers + skill loader + MCP tools);
    /// injected into every execution through the trigger listener.
    pub tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    /// Shared MCP connection manager; `None` when MCP is not configured.
    pub mcp_manager: Option<Arc<wf_tools::mcp::connection::McpConnectionManager>>,
    pub event_bus: Arc<EventBus>,
    pub metrics: Option<Arc<MetricsContext>>,
    pub llm_gateway: Arc<LlmGateway>,
    /// Variable maps of live workflow executions (write-back target of the
    /// event-driven context compression chain).
    pub execution_contexts: Arc<ExecutionContextRegistry>,
    /// Background event-driven trigger listener (context compression).
    pub trigger_listener: Option<Arc<TriggerEventListener>>,
    trigger_listener_shutdown: Option<tokio_util::sync::CancellationToken>,
    trigger_listener_handle: Option<tokio::task::JoinHandle<()>>,
    #[cfg(feature = "plugins")]
    pub plugin_engine: Option<wf_plugin::PluginEngine>,
}

impl Runtime {
    pub async fn bootstrap(config: RuntimeConfig) -> RuntimeResult<Self> {
        let mode_info = detect_all(config.mode_override);
        let effective_log_config = adjust_log_config(config.log_config, &mode_info);

        let _guard = init_tracing(&effective_log_config)?;

        info!("Bootstrapping runtime in {:?} mode", mode_info.mode);

        let mut storage_manager = StorageManager::new(config.storage);
        storage_manager.initialize().await?;

        let registries = Arc::new(Registries::new());
        let bundles = Arc::new(BundleRegistry::new());

        let skill_loader = Arc::new(wf_tools::SkillLoader::new(config.skills));
        let skill_count = skill_loader.list_skills().len();
        if skill_count > 0 {
            info!("Skill registry initialized: {} skills", skill_count);
        }

        // MCP: load merged settings (global + project), register servers and
        // connect eager/keep-alive ones. Lazy servers connect on first use.
        let mcp_manager = init_mcp(&config.mcp).await;
        if let Some(manager) = &mcp_manager {
            info!(
                "MCP manager initialized: {} servers registered",
                manager.registry().list().len()
            );
        }

        // Shared tool registry: builtin handlers + skill loader + MCP tools.
        let tool_registry = Arc::new(wf_tools::create_default_tool_registry());
        tool_registry.set_skill_loader(skill_loader.clone());
        if let Some(manager) = &mcp_manager {
            tool_registry.set_mcp_manager(manager.clone());
            let registry = tool_registry.clone();
            let manager_clone = manager.clone();
            manager.set_on_connected(Arc::new(move |_server| {
                wf_tools::mcp::registration::register_connected_tools(
                    &registry,
                    &manager_clone,
                );
            }));
            wf_tools::mcp::registration::register_use_mcp(&tool_registry).map_err(|e| {
                crate::error::RuntimeError::Config(format!("Failed to register use_mcp: {}", e))
            })?;
            wf_tools::mcp::registration::register_connected_tools(&tool_registry, manager);
        }

        let resource_result =
            wf_resource::register_all(&registries, &bundles, &config.resource.options);
        info!(
            "Resource registration: {} succeeded, {} failed",
            resource_result.succeeded.len(),
            resource_result.failed.len(),
        );
        for fail in &resource_result.failed {
            tracing::warn!("Resource registration failed: {} - {}", fail.id, fail.error);
        }

        let event_bus = Arc::new(EventBus::new(1024));
        let metrics = match config.metrics.as_ref() {
            Some(cfg) => {
                // Share one config collector between the merge path (records
                // access) and the metrics registry (exposes the counters).
                let config_metrics = Arc::new(wf_metrics::ConfigMetricsCollector::new(
                    wf_metrics::CollectorConfig::default(),
                ));
                let merged = merge_metrics_with_defaults(cfg, Some(&config_metrics));
                MetricsContext::start(
                    &merged,
                    &storage_manager,
                    Some(event_bus.clone()),
                    Some(config_metrics),
                )
                .await?
            }
            None => None,
        };
        if metrics.is_some() {
            info!("Metrics system initialized");
        }

        let (shutdown_handle, _shutdown_waiter) = shutdown_channel();

        #[cfg(feature = "plugins")]
        let plugin_engine = init_plugins(&config.plugins).await?;

        let llm_gateway =
            init_llm_gateway(&config.llm, metrics.as_ref().map(|m| m.registry().as_ref()))?;

        // Event-driven trigger listener: powers the context-compression chain
        // (CONTEXT_COMPRESSION_REQUESTED -> llm_summary_workflow -> write-back).
        let execution_contexts = Arc::new(ExecutionContextRegistry::new());
        let listener = start_trigger_listener_with_registry(
            event_bus.clone(),
            registries.clone(),
            llm_gateway.clone(),
            execution_contexts.clone(),
            Some(tool_registry.clone()),
        );

        info!("Runtime bootstrap complete");

        Ok(Self {
            storage_manager,
            mode_info,
            shutdown_handle,
            _shutdown_waiter,
            registries,
            bundles,
            skill_loader,
            tool_registry,
            mcp_manager,
            event_bus,
            metrics,
            llm_gateway,
            execution_contexts,
            trigger_listener: Some(listener.listener),
            trigger_listener_shutdown: Some(listener.shutdown),
            trigger_listener_handle: Some(listener.handle),
            #[cfg(feature = "plugins")]
            plugin_engine,
        })
    }

    pub fn registries(&self) -> &Registries {
        &self.registries
    }

    pub fn bundles(&self) -> &BundleRegistry {
        &self.bundles
    }

    /// Shared skill loader; skills are scanned from configured paths at bootstrap.
    pub fn skill_loader(&self) -> &Arc<wf_tools::SkillLoader> {
        &self.skill_loader
    }

    /// Shared tool registry; injected into executions started by this runtime.
    pub fn tool_registry(&self) -> &Arc<wf_tools::registry::ToolRegistry> {
        &self.tool_registry
    }

    /// Shared MCP connection manager, when MCP settings were configured.
    pub fn mcp_manager(&self) -> Option<&Arc<wf_tools::mcp::connection::McpConnectionManager>> {
        self.mcp_manager.as_ref()
    }

    pub async fn shutdown(mut self) -> RuntimeResult<()> {
        if let Some(metrics) = self.metrics.take() {
            metrics.shutdown().await;
        }

        if let Some(handle) = self.trigger_listener_handle.take() {
            if let Some(token) = self.trigger_listener_shutdown.take() {
                token.cancel();
            }
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
            info!("Trigger listener stopped");
        }

        if let Some(manager) = self.mcp_manager.take() {
            let servers = manager.connected_servers();
            for server in servers {
                let _ = manager.disconnect(&server).await;
            }
            info!("MCP connections closed");
        }

        #[cfg(feature = "plugins")]
        if let Some(engine) = self.plugin_engine.take() {
            engine.shutdown().await;
        }

        self.storage_manager.close().await?;
        info!("Runtime shutdown complete");
        Ok(())
    }

    pub fn storage(&self) -> &StorageManager {
        &self.storage_manager
    }

    pub fn mode(&self) -> &ModeInfo {
        &self.mode_info
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_handle.is_shutting_down()
    }

    pub fn trigger_shutdown(&self) {
        self.shutdown_handle.trigger();
    }

    /// Optional metrics system; absent when metrics are disabled.
    pub fn metrics(&self) -> Option<&Arc<MetricsContext>> {
        self.metrics.as_ref()
    }

    /// Shared LLM gateway; workflow and agent execution are injected with
    /// this instance so all LLM calls resolve profiles from one registry.
    pub fn llm_gateway(&self) -> &Arc<LlmGateway> {
        &self.llm_gateway
    }

    #[cfg(feature = "plugins")]
    pub fn plugin_engine(&self) -> Option<&wf_plugin::PluginEngine> {
        self.plugin_engine.as_ref()
    }
}

/// Build the shared LLM gateway: validate and register every configured
/// profile (wf-config llm_profile processors), then attach the runtime token
/// metrics collector when metrics are enabled.
fn init_llm_gateway(
    config: &LlmConfig,
    metrics: Option<&wf_metrics::MetricsRegistry>,
) -> RuntimeResult<Arc<LlmGateway>> {    let mut gateway = LlmGateway::new();

    for profile in &config.profiles {
        validate_llm_profile(profile).map_err(|e| {
            crate::error::RuntimeError::Config(format!("Invalid LLM profile: {}", e))
        })?;
        let transformed = transform_llm_profile(profile, &std::collections::HashMap::new())
            .map_err(|e| {
                crate::error::RuntimeError::Config(format!("Invalid LLM profile: {}", e))
            })?;
        gateway.register_profile(transformed).map_err(|e| {
            crate::error::RuntimeError::Config(format!("Failed to register LLM profile: {}", e))
        })?;
    }

    if let Some(registry) = metrics {
        gateway = gateway.with_token_metrics(registry.token().as_ref().clone());
    }

    Ok(Arc::new(gateway))
}

/// Build the MCP connection manager from merged settings. Returns `None`
/// when MCP is not configured (no settings sources or no servers). Servers
/// are registered with their configured lifecycle; eager/keep-alive servers
/// are connected immediately (failure is logged, not fatal).
async fn init_mcp(config: &McpRuntimeConfig) -> Option<Arc<wf_tools::mcp::connection::McpConnectionManager>> {
    use wf_tools::mcp::connection::{McpConnectionManager, McpServerRegistry};

    let (Some(settings_dir), Some(project_root)) = (&config.settings_dir, &config.project_root)
    else {
        return None;
    };

    let settings = wf_config::mcp::load_and_merge_mcp_settings(settings_dir, project_root)
        .unwrap_or_default();
    if settings.mcp_servers.is_empty() {
        return None;
    }

    let registry = Arc::new(McpServerRegistry::new());
    let manager = Arc::new(McpConnectionManager::new(registry));
    for (name, server_config) in &settings.mcp_servers {
        if let Err(e) = manager.connect_server(name, server_config.clone()).await {
            tracing::warn!("MCP server '{}' failed to connect: {}", name, e);
        }
    }
    Some(manager)
}

#[cfg(feature = "plugins")]
async fn init_plugins(config: &PluginConfig) -> RuntimeResult<Option<wf_plugin::PluginEngine>> {
    if !config.enabled {
        return Ok(None);
    }

    let plugin_config = wf_plugin::PluginSystemConfig {
        enabled: true,
        paths: config.paths.clone(),
        auto_activate: config.auto_activate,
        guard_timeout_ms: config.guard_timeout_ms,
        ..Default::default()
    };

    let registry = Arc::new(wf_plugin::PluginRegistry::new());
    let contribution_manager = Arc::new(wf_plugin::ContributionManager::new());
    let bridge: Option<Arc<dyn wf_plugin::ContributionBridge>> =
        Some(Arc::new(crate::plugin_bridge::WfPluginBridge));

    let event_bus = wf_core::EventBus::new(256);

    let mut engine = wf_plugin::PluginEngine::new(
        registry,
        contribution_manager,
        bridge,
        plugin_config,
        env!("CARGO_PKG_VERSION"),
    )
    .with_event_bus(event_bus);

    engine.initialize().await.map_err(|e| {
        tracing::error!("Plugin engine initialization failed: {}", e);
        crate::error::RuntimeError::Config(format!("Plugin init failed: {}", e))
    })?;

    Ok(Some(engine))
}

fn adjust_log_config(mut config: LogConfig, mode_info: &ModeInfo) -> LogConfig {
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
    use crate::mode::ExecutionMode;
    use crate::storage_manager::StorageBackendType;
    use wf_core::registry::Registry;

    fn clear_env_vars() {
        std::env::remove_var("CLI_MODE");
        std::env::remove_var("HEADLESS");
        std::env::remove_var("TEST_MODE");
        std::env::remove_var("CLI_OUTPUT_FORMAT");
        std::env::remove_var("NO_COLOR");
    }

    #[tokio::test]
    async fn test_runtime_bootstrap_memory() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                backend_type: StorageBackendType::Memory,
                ..Default::default()
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            metrics: None,
            llm: LlmConfig::default(),
            skills: Default::default(),
            mcp: Default::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        assert!(runtime.storage().is_initialized());
        assert!(runtime.mode().is_test());
        assert!(!runtime.registries().tools.is_empty());

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_registries_populated() {
        clear_env_vars();

        let config = RuntimeConfig {
            log_config: LogConfig::default().with_level("off"),
            resource: ResourceConfig::default(),
            metrics: None,
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        // Verify registries are populated from register_all()
        assert!(!runtime.registries().tools.is_empty());
        assert!(!runtime.registries().fragments.is_empty());
        assert!(!runtime.registries().prompt_templates.is_empty());
        assert!(!runtime.registries().tool_descriptions.is_empty());
        assert!(!runtime.registries().agent_templates.is_empty());
        assert!(!runtime.registries().trigger_templates.is_empty());
        assert!(!runtime.registries().workflows.is_empty());

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_goal_review_starter_activation() {
        clear_env_vars();

        use wf_resource::registrar::{Options as ResourceOptions, StarterActivation};
        use wf_workflow::validation::GraphValidator;

        let config = RuntimeConfig {
            log_config: LogConfig::default().with_level("off"),
            resource: ResourceConfig {
                options: ResourceOptions {
                    starter_activation: vec![StarterActivation {
                        id: "@standard/goal-review-agent".into(),
                        config: serde_json::json!({
                            "root_requirement": "fix the failing test",
                            "max_iterations": 3,
                        }),
                    }],
                    ..Default::default()
                },
            },
            metrics: None,
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        // Built-in starter registered and activated: workflow + planner prompt.
        let bundles = runtime.bundles();
        assert!(bundles
            .list()
            .contains(&"@standard/goal-review-agent".to_string()));
        assert!(bundles.is_active("@standard/goal-review-agent"));

        assert!(runtime
            .registries()
            .workflows
            .has("@standard/goal-review-agent-workflow"));
        assert!(runtime
            .registries()
            .prompt_templates
            .has("@standard/goal-review-planner"));

        // The assembled workflow is structurally valid (loop pairs, edges,
        // reachability) so it can be executed by the workflow engine.
        let wf = runtime
            .registries()
            .workflows
            .get("@standard/goal-review-agent-workflow")
            .expect("goal review workflow registered");
        let graph = crate::trigger_listener::template_to_graph(&wf);
        GraphValidator::validate(&graph).unwrap_or_else(|errors| {
            panic!(
                "goal review workflow failed validation: {:?}",
                errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
            )
        });

        // Deactivation removes the workflow and prompt from the registries.
        bundles
            .deactivate("@standard/goal-review-agent", runtime.registries())
            .unwrap();
        assert!(!bundles.is_active("@standard/goal-review-agent"));
        assert!(!runtime
            .registries()
            .workflows
            .has("@standard/goal-review-agent-workflow"));
        assert!(!runtime
            .registries()
            .prompt_templates
            .has("@standard/goal-review-planner"));

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_trigger_shutdown() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                backend_type: StorageBackendType::Memory,
                ..Default::default()
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            metrics: None,
            llm: LlmConfig::default(),
            skills: Default::default(),
            mcp: Default::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        assert!(!runtime.is_shutting_down());
        runtime.trigger_shutdown();
        assert!(runtime.is_shutting_down());

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert!(matches!(
            config.storage.backend_type,
            StorageBackendType::Memory
        ));
        assert!(config.mode_override.is_none());
        assert!(config.metrics.is_none());
    }

    #[tokio::test]
    async fn test_runtime_metrics_wiring() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                backend_type: StorageBackendType::Memory,
                ..Default::default()
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            skills: Default::default(),
            mcp: Default::default(),
            metrics: Some(wf_types::config::metrics::MetricsConfig {
                workflow_metrics: Some(wf_types::config::metrics::MetricCollectorConfig {
                    flush_interval: Some(100),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            llm: LlmConfig::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        let metrics = runtime
            .metrics()
            .expect("metrics system should be initialized");
        metrics
            .registry()
            .workflow()
            .record_execution_start("exec-1", "wf-1");
        assert_eq!(metrics.registry().workflow().usage_stats().total, 1);

        // Background flush task persists buffered metrics into storage.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        use wf_storage::adapter::metrics::MetricsStorageAdapter;
        let loaded = runtime
            .storage()
            .context()
            .unwrap()
            .metrics
            .query("workflow.execution.count", 0, wf_common::now() + 1000)
            .await
            .unwrap();
        assert_eq!(loaded.len(), 1);

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_metrics_disabled() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                backend_type: StorageBackendType::Memory,
                ..Default::default()
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            skills: Default::default(),
            mcp: Default::default(),
            metrics: Some(wf_types::config::metrics::MetricsConfig {
                enabled: Some(false),
                ..Default::default()
            }),
            llm: LlmConfig::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();
        assert!(runtime.metrics().is_none());

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[test]
    fn test_adjust_log_config_json_mode() {
        use crate::logger::LogFormat;

        let mode = ModeInfo {
            mode: ExecutionMode::Headless,
            output_format: crate::mode::OutputFormat::Json,
            color_enabled: false,
        };

        let config = LogConfig::default().with_format(LogFormat::Full);
        let adjusted = adjust_log_config(config, &mode);

        assert_eq!(adjusted.format, LogFormat::Json);
    }

    #[test]
    fn test_adjust_log_config_silent_mode() {
        let mode = ModeInfo {
            mode: ExecutionMode::Interactive,
            output_format: crate::mode::OutputFormat::Silent,
            color_enabled: false,
        };

        let config = LogConfig::default().with_level("info");
        let adjusted = adjust_log_config(config, &mode);

        assert_eq!(adjusted.level, "off");
    }

    #[tokio::test]
    async fn test_bootstrap_registers_mcp_manager_and_use_mcp() {
        clear_env_vars();

        let root = std::env::temp_dir().join(format!("wf-runtime-mcp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // A lazy server: registered but not connected, so no process spawn.
        std::fs::write(
            root.join("mcp-settings.json"),
            r#"{"mcpServers": {"echo-srv": {"type": "stdio", "command": "echo", "timeout": 5}}}"#,
        )
        .unwrap();

        let config = RuntimeConfig {
            storage: StorageConfig {
                backend_type: StorageBackendType::Memory,
                ..Default::default()
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            skills: Default::default(),
            mcp: McpRuntimeConfig {
                settings_dir: Some(root.clone()),
                project_root: Some(root.clone()),
            },
            metrics: None,
            llm: LlmConfig::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        let manager = runtime.mcp_manager().expect("MCP manager initialized");
        assert_eq!(manager.registry().list().len(), 1);
        assert!(manager.connected_servers().is_empty(), "lazy server not connected");

        // use_mcp is registered into the shared tool registry.
        assert!(runtime.tool_registry().get_tool("use_mcp").is_some());

        runtime.shutdown().await.unwrap();
        let _ = std::fs::remove_dir_all(&root);
        clear_env_vars();
    }

    #[test]
    fn test_llm_gateway_registers_and_rejects_invalid_profiles() {
        let profiles = vec![wf_types::llm::LlmProfile {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            provider: wf_types::llm::LlmProvider::OpenaiChat,
            model: "gpt-4o".to_string(),
            api_key: None,
            base_url: None,
            parameters: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_format: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
        }];
        let gateway = init_llm_gateway(&LlmConfig { profiles }, None).unwrap();
        assert!(gateway.has_profile("openai"));

        let err = match init_llm_gateway(
            &LlmConfig {
                profiles: vec![wf_types::llm::LlmProfile {
                    id: String::new(),
                    name: "broken".to_string(),
                    provider: wf_types::llm::LlmProvider::OpenaiChat,
                    model: String::new(),
                    api_key: None,
                    base_url: None,
                    parameters: None,
                    timeout: None,
                    max_retries: None,
                    retry_delay: None,
                    headers: None,
                    metadata: None,
                    tool_call_format: None,
                    auth_type: None,
                    custom_headers: None,
                    custom_body: None,
                    custom_body_enabled: None,
                    query_params: None,
                    stream_options: None,
                }],
            },
            None,
        ) {
            Ok(_) => panic!("invalid profile must be rejected"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("LLM profile"));
    }
}
