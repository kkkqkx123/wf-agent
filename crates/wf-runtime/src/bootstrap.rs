use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info, warn};
use wf_workflow::trigger_listener::TriggerEventListener;

use wf_config::orchestrator::{default_infra_file_mapping, ConfigOrchestrator};
use wf_config::processor::infrastructure::merge_metrics_with_defaults;
use wf_config::processor::llm_profile::{transform_llm_profile, validate_llm_profile};
use wf_core::event::EventBus;
use wf_execution_shared::hooks::HookRegistry;
use wf_llm::LlmGateway;
use wf_resource::registry::{RegisterOptions as ResourceOptions, ResourceRegistries};
use wf_resource::resource_plugin::ResourcePluginRegistry;
use wf_types::config::file_checkpoint::FileCheckpointConfig;
use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::presets::PresetsConfig;
use wf_types::config::timeout::TimeoutConfig;
use wf_types::llm::LlmProfile;

use crate::error::RuntimeResult;
use crate::lifecycle::{shutdown_channel, ShutdownHandle, ShutdownWaiter};
use crate::logger::{init_tracing, LogConfig};
use crate::metrics::MetricsContext;
use crate::mode::{detect_all, ModeInfo};
use crate::storage_manager::StorageManager;
use crate::trigger_listener::{
    register_compression_receiver, start_trigger_listener_with_parts, ExecutionContextRegistry,
    TriggerExecutionRecorder, WorkflowRunner,
};
use wf_api::PersistenceLayer as ApiPersistenceLayer;
use wf_types::config::storage::StorageConfig;

#[derive(Debug, Clone, Default)]
pub struct ResourceConfig {
    pub options: ResourceOptions,
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
    pub overrides: wf_config::orchestrator::ConfigOverrides,
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
    /// Shell tool configuration; when `output_event_enabled` is set, shell
    /// session/output events are bridged to the runtime `EventBus`.
    pub shell: wf_shell::config::ShellToolConfig,
    /// Global sandbox configuration (profiles + routing rules). Compiled and
    /// validated at bootstrap (fail-fast); the resulting shared runtime is
    /// exposed via [`Runtime::sandbox_runtime`] and injected into every
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
    /// File-layer infrastructure config source; `None` keeps the runtime
    /// programmatic-only (storage/metrics/sandbox defaults).
    pub infra: Option<InfraSourceConfig>,
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
    pub registries: Arc<ResourceRegistries>,
    pub bundles: Arc<ResourcePluginRegistry>,
    pub skill_loader: Arc<wf_tools::SkillLoader>,
    /// Shared tool registry (builtin handlers + skill loader + MCP tools);
    /// injected into every execution through the trigger listener.
    pub tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    /// Shared MCP connection manager; `None` when MCP is not configured.
    pub mcp_manager: Option<Arc<wf_tools::mcp::connection::McpConnectionManager>>,
    pub event_bus: Arc<EventBus>,
    pub metrics: Option<Arc<MetricsContext>>,
    pub llm_gateway: Arc<LlmGateway>,
    /// Shared sandbox runtime: global profiles and routing rules compiled
    /// at bootstrap, injected into every script handler.
    pub sandbox_runtime: Arc<wf_sandbox::SandboxRuntime>,
    /// Variable maps of live workflow executions (write-back target of the
    /// event-driven context compression chain).
    pub execution_contexts: Arc<ExecutionContextRegistry>,
    /// Background event-driven trigger listener (context compression).
    pub trigger_listener: Option<Arc<TriggerEventListener>>,
    trigger_listener_shutdown: Option<tokio_util::sync::CancellationToken>,
    trigger_listener_handle: Option<tokio::task::JoinHandle<()>>,
    /// Trigger runtime state registry: the listener records fired triggers
    /// here and checkpoints capture them as the `trigger_states` audit trail.
    pub trigger_state_registry: Arc<wf_workflow::TriggerStateRegistry>,
    /// Shared hook receiver registry: engine hook points and signals
    /// (context compression) dispatch through it; the compression service is
    /// registered on the `CONTEXT_COMPRESSION_REQUESTED` signal point.
    pub hook_registry: Arc<HookRegistry>,
    /// Shared agent loop registry of the composite execution callback;
    /// injected into the API context so tool-dispatched executions appear in
    /// the server execution views.
    agent_registry: std::sync::Arc<wf_agent::registry::AgentLoopRegistry>,
    #[cfg(feature = "plugins")]
    pub plugin_engine: Option<wf_plugin::PluginEngine>,
    /// Durable event persistence backend (buffered + store mirroring the
    /// runtime storage). `None` keeps events in memory only.
    event_persistence: Option<Arc<dyn wf_api::PersistenceLayer>>,
    /// Durable checkpoint store backend (crash recovery): execution
    /// checkpoints written through `ApiContext::checkpoint_store` land in the
    /// same backend as the runtime storage. Falls back to in-memory when
    /// storage is memory-only or the backend cannot be opened.
    checkpoint_store: Arc<wf_storage::backend::StorageBackend>,
    /// Lazily-created application-facing API context; shared so live execution
    /// handles (pause/resume/cancel) stay valid across calls.
    api_ctx: std::sync::OnceLock<wf_api::ApiContext>,
    /// File checkpoint manager (layertwine-backed): execution file snapshots
    /// are created/restored through it and script handlers capture workspace
    /// changes when it is attached. `None` keeps file checkpointing disabled.
    #[cfg(feature = "checkpoint")]
    file_checkpoint_manager: Option<wf_checkpoint::file::FileCheckpointManager>,
    /// Manual change service: watches the workspace root and routes
    /// human/external file edits into the manual partition. Started when
    /// file checkpointing is enabled with a workspace root and `manual_watch`.
    #[cfg(feature = "checkpoint")]
    manual_change_service: Option<wf_checkpoint::watcher::ManualChangeService>,
    /// Forwarder task from the file-checkpoint event bus onto the shared
    /// event bus (CheckpointFileChanged / CheckpointMergeConflicted with the
    /// `DeltaSummary` payload). Kept alive for the runtime lifetime.
    #[cfg(feature = "checkpoint")]
    checkpoint_event_bridge_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Assembled event-driven trigger subsystem produced by
/// [`assemble_trigger_subsystem`]: the listener handle plus the shared
/// write-back registries both the listener and the builtin compression
/// receiver operate on.
struct TriggerSubsystem {
    execution_contexts: Arc<ExecutionContextRegistry>,
    trigger_state_registry: Arc<wf_workflow::TriggerStateRegistry>,
    listener: crate::trigger_listener::TriggerListenerHandle,
}

/// Assemble the event-driven trigger subsystem in one step.
///
/// Wires the trigger listener (powers the nested-agent-execution action
/// `HookTriggered` etc. and user trigger templates) and the builtin
/// context-compression hook receiver together: both share the same
/// sub-workflow runner, shutdown token, execution-context registry and
/// trigger-state registry so engine compression signals and user triggers
/// run over one consistent lifecycle. Trigger executions are recorded in
/// the durable ledger when storage is available (management surface).
#[allow(clippy::too_many_arguments)]
fn assemble_trigger_subsystem(
    registries: Arc<ResourceRegistries>,
    event_bus: Arc<EventBus>,
    llm_gateway: Arc<LlmGateway>,
    tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    sandbox_runtime: Arc<wf_sandbox::SandboxRuntime>,
    agent_executor: Arc<wf_agent::executor::AgentLoopExecutor>,
    hook_registry: Arc<HookRegistry>,
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
) -> TriggerSubsystem {
    let execution_contexts = Arc::new(ExecutionContextRegistry::new());
    let trigger_state_registry = Arc::new(wf_workflow::TriggerStateRegistry::new());
    let trigger_shutdown = tokio_util::sync::CancellationToken::new();
    let subworkflow_runner: std::sync::Arc<dyn wf_workflow::trigger_listener::SubworkflowRunner> =
        std::sync::Arc::new(WorkflowRunner::with_tool_registry(
            registries.clone(),
            event_bus.clone(),
            llm_gateway.clone(),
            execution_contexts.clone(),
            Some(tool_registry.clone()),
            Some(sandbox_runtime.clone()),
        ));
    let listener = start_trigger_listener_with_parts(
        event_bus.clone(),
        registries.clone(),
        execution_contexts.clone(),
        subworkflow_runner.clone(),
        llm_gateway.clone(),
        Some(tool_registry.clone()),
        Some(sandbox_runtime.clone()),
        Some(agent_executor.clone()),
        storage.clone(),
        Some(trigger_state_registry.clone()),
        Some(hook_registry.clone()),
        trigger_shutdown.clone(),
    );
    // The builtin compression receiver shares the listener's shutdown token
    // and sub-workflow runner: engine signals dispatch to it, the summary
    // sub-workflow is spawned immediately and stopped at runtime shutdown
    // together with the listener.
    let _compression = register_compression_receiver(
        &hook_registry,
        event_bus,
        subworkflow_runner,
        execution_contexts.clone(),
        wf_resource::predefined::workflow::LLM_SUMMARY_WORKFLOW_ID.to_string(),
        trigger_shutdown,
        storage,
        Some(trigger_state_registry.clone()),
    );
    TriggerSubsystem {
        execution_contexts,
        trigger_state_registry,
        listener,
    }
}

impl Runtime {
    pub async fn bootstrap(mut config: RuntimeConfig) -> RuntimeResult<Self> {
        // File-layer infrastructure resolution: fill storage / timeout /
        // metrics / output / sandbox / presets / tools / file_checkpoint
        // from the orchestrator-assembled config, plus the skill settings
        // chain. Programmatic values always win (file layer is the default
        // source only).
        if let Some(infra) = config.infra.clone() {
            config = resolve_infra_config(config, &infra).await?;
        }

        let mode_info = detect_all(config.mode_override);
        let effective_log_config = adjust_log_config(config.log_config, &mode_info);

        let _guard = init_tracing(&effective_log_config)?;

        info!("Bootstrapping runtime in {:?} mode", mode_info.mode);

        // Durable event persistence backend: engine events published
        // on the shared bus are buffered and flushed to the same backend as
        // the runtime storage, so history survives restarts. `None` (memory
        // or postgres storage, or a failed open) keeps events in memory only.
        let event_persistence = init_event_persistence(&config.storage).await;

        // Durable checkpoint store backend: shares the storage backend
        // so execution checkpoints survive restarts.
        let checkpoint_store = init_checkpoint_store(&config.storage).await;

        let mut storage_manager = StorageManager::new(config.storage);
        storage_manager.initialize().await?;

        let registries = Arc::new(ResourceRegistries::new());
        let bundles = Arc::new(ResourcePluginRegistry::new());

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

        // Shared event bus: shell event bridge depends on it, so it is
        // created before the tool registry.
        let event_bus = Arc::new(EventBus::new(1024));

        // Shared hook receiver registry: hook points and engine signals
        // (context compression) dispatch through it. The builtin compression
        // receiver is registered once the execution write-back registry and
        // the trigger shutdown token exist (below).
        let hook_registry = Arc::new(HookRegistry::new());

        // Shared sandbox runtime: compile the global config (profiles +
        // routing rules) up front so configuration errors surface at
        // bootstrap, not at script execution. Created before the tool
        // registry so its default policy can harden every external command
        // (shell tool, CLI executors) through the shared execution gateway.
        let sandbox_runtime = Arc::new(match &config.sandbox {
            Some(global) => wf_sandbox::SandboxRuntime::with_global_config(global.clone())
                .map_err(|e| {
                    crate::error::RuntimeError::Config(format!(
                        "Invalid sandbox global config: {e}"
                    ))
                })?,
            None => wf_sandbox::SandboxRuntime::new(),
        });
        if let Some(global) = &config.sandbox {
            info!(
                "Sandbox runtime initialized: {} profiles, {} routing rules",
                global.profiles.len(),
                global.rules.len()
            );
        }

        // Shared tool registry: builtin handlers + skill loader + MCP tools.
        // When shell output events are enabled, a bridge forwards them to
        // the shared EventBus (unless a custom sink is already configured).
        // The global sandbox default policy is attached to the shell config
        // so execute_command/session commands are hardened by the same
        // seccomp gateway as script nodes.
        let mut shell_config = config.shell.clone();
        if shell_config.output_event_enabled && shell_config.event_sink.is_none() {
            shell_config.event_sink = Some(Arc::new(
                crate::shell_event_bridge::ShellEventBusBridge::new(event_bus.clone()),
            ));
        }
        shell_config.sandbox_policy = Some(sandbox_runtime.default_policy().clone());
        let tool_registry = Arc::new(wf_tools::registry::ToolRegistry::new());
        wf_tools::register_builtin_handlers(
            &tool_registry,
            wf_tools::BuiltinHandlersConfig {
                shell: shell_config,
                ..Default::default()
            },
        )
        .map_err(|e| {
            crate::error::RuntimeError::Config(format!(
                "Failed to register builtin handlers: {}",
                e
            ))
        })?;
        tool_registry.set_skill_loader(skill_loader.clone());
        if let Some(manager) = &mcp_manager {
            tool_registry.set_mcp_manager(manager.clone());
            let registry = tool_registry.clone();
            let manager_clone = manager.clone();
            manager.set_on_connected(Arc::new(move |_server| {
                wf_tools::mcp::registration::register_connected_tools(&registry, &manager_clone);
            }));
            wf_tools::mcp::registration::register_use_mcp(&tool_registry).map_err(|e| {
                crate::error::RuntimeError::Config(format!("Failed to register use_mcp: {}", e))
            })?;
            wf_tools::mcp::registration::register_connected_tools(&tool_registry, manager);
        }

        // The plugin engine starts before resource registration so built-in
        // resource plugins activate through the unified engine and land in
        // the registries via the contribution bridge. When the engine is
        // disabled the legacy `ResourcePluginRegistry` path is used so
        // built-in resource plugins (e.g. goal-review) keep working without
        // the plugin system.
        #[cfg(feature = "plugins")]
        let plugin_engine =
            init_plugins(&config.plugins, registries.clone(), tool_registry.clone()).await?;
        #[cfg(feature = "plugins")]
        match &plugin_engine {
            Some(engine) => {
                crate::resource_plugin_adapter::activate_builtin_resource_plugins_via_engine(
                    engine,
                    &config.resource.options,
                )
                .await?
            }
            None => activate_builtin_resource_plugins_legacy(
                &bundles,
                &config.resource.options,
                &registries,
                &tool_registry,
            )?,
        };
        #[cfg(not(feature = "plugins"))]
        activate_builtin_resource_plugins_legacy(
            &bundles,
            &config.resource.options,
            &registries,
            &tool_registry,
        )?;

        let resource_result =
            wf_resource::register_all(&registries, &tool_registry, &config.resource.options);
        info!(
            "Resource registration: {} succeeded, {} failed",
            resource_result.succeeded.len(),
            resource_result.failed.len(),
        );
        for fail in &resource_result.failed {
            tracing::warn!("Resource registration failed: {} - {}", fail.id, fail.error);
        }

        // The agent loop registry is created before metrics so the runtime
        // can wire its capacity gate into the resource sampler from the
        // first observation tick.
        let agent_registry = std::sync::Arc::new(wf_agent::registry::AgentLoopRegistry::new());
        let gate_stats = agent_registry.gate_stats();
        info!(
            "Agent capacity gate at startup: max_concurrent={}, active={}, available={}",
            gate_stats.max_concurrent,
            gate_stats.active_count,
            gate_stats.available_permits
        );

        let metrics = match config.metrics.as_ref() {
            Some(cfg) => {
                let config_metrics = Arc::new(wf_metrics::ConfigMetricsCollector::new(
                    wf_metrics::CollectorConfig::default(),
                ));
                let merged = merge_metrics_with_defaults(cfg);
                MetricsContext::start(
                    &merged,
                    &storage_manager,
                    Some(event_bus.clone()),
                    Some(config_metrics),
                    Some(agent_registry.capacity_gate()),
                )
                .await?
            }
            None => None,
        };
        if metrics.is_some() {
            info!("Metrics system initialized");
        }

        let (shutdown_handle, _shutdown_waiter) = shutdown_channel();

        let llm_gateway =
            init_llm_gateway(&config.llm, metrics.as_ref().map(|m| m.registry().as_ref()))?;

        // Execution callback assembly: a composite covering agent and
        // workflow dispatch, registered on both the global callback
        // singleton and the shared tool registry. Fixes the production path
        // where builtin dispatch tools previously failed with
        // CallbackNotRegistered.
        let agent_executor = std::sync::Arc::new(
            wf_agent::executor::AgentLoopExecutor::new(llm_gateway.clone(), tool_registry.clone())
                .with_shared_registry(agent_registry.clone())
                .with_hook_registry(hook_registry.clone()),
        );
        let mut workflow_callback =
            wf_workflow::execution_callback::WorkflowExecutionCallback::new(tool_registry.clone())
                .with_gateway(llm_gateway.clone())
                .with_event_bus(event_bus.clone())
                .with_sandbox(sandbox_runtime.clone())
                .with_hook_registry(hook_registry.clone());
        if let Some(metrics) = metrics.as_ref() {
            workflow_callback = workflow_callback.with_metrics(metrics.registry().clone());
        }
        let workflow_callback = Arc::new(workflow_callback);
        // Register every registered workflow template so the execute_workflow
        // tool can resolve it at runtime. Definition-level hooks travel with
        // the template and are executed per node (BEFORE_EXECUTE /
        // AFTER_EXECUTE).
        for id in wf_core::registry::Registry::list(&registries.workflows) {
            if let Some(template) = wf_core::registry::Registry::get(&registries.workflows, &id) {
                let graph = crate::trigger_listener::template_to_graph(&template);
                let hooks = template
                    .definition
                    .hooks
                    .as_ref()
                    .map(|hooks| hooks.iter().map(Into::into).collect())
                    .unwrap_or_default();
                workflow_callback.register_workflow_with_hooks(
                    wf_types::Id::from(id.clone()),
                    graph,
                    hooks,
                );
            }
        }
        let composite = std::sync::Arc::new(
            crate::execution_callback::CompositeExecutionCallback::new()
                .with_agent(agent_executor.clone())
                .with_workflow(workflow_callback),
        );
        wf_tools::callback::register_execution_callback(composite.clone()).map_err(|e| {
            crate::error::RuntimeError::Config(format!(
                "Failed to register execution callback: {}",
                e
            ))
        })?;
        tool_registry.set_builtin_callback(composite);

        // Tool persistence bridge: hydrate tools registered at runtime in a
        // previous run from the durable backend. A no-op for memory storage
        // (nothing was persisted) and non-fatal when the backend fails.
        if let Some(ctx) = storage_manager.shared_context() {
            let bridge = crate::tool_storage::StorageToolBridge::new(ctx.tool_definition.clone());
            match tool_registry.initialize_from_storage(&bridge).await {
                Ok(()) => {
                    if tool_registry.tool_count() > 0 {
                        info!(
                            "Tool registry hydrated from storage: {} persisted tools",
                            tool_registry.tool_count()
                        );
                    }
                }
                Err(err) => {
                    warn!(error = %err, "failed to restore persisted tools; registry continues with runtime-registered tools");
                }
            }
        }

        // Event-driven trigger subsystem: powers the nested-agent-execution
        // action (HookTriggered etc.) and user trigger templates. The context
        // compression chain is now served by the hook registry: the engine
        // dispatches the CONTEXT_COMPRESSION_REQUESTED signal synchronously
        // and the compression receiver (assembled below) takes over
        // immediately.
        let trigger_subsystem = assemble_trigger_subsystem(
            registries.clone(),
            event_bus.clone(),
            llm_gateway.clone(),
            tool_registry.clone(),
            sandbox_runtime.clone(),
            agent_executor.clone(),
            hook_registry.clone(),
            storage_manager.shared_context().map(|ctx| {
                Arc::new(ctx.trigger_execution.clone()) as Arc<dyn TriggerExecutionRecorder>
            }),
        );
        let execution_contexts = trigger_subsystem.execution_contexts;
        let trigger_state_registry = trigger_subsystem.trigger_state_registry;
        let listener = trigger_subsystem.listener;

        // File checkpoint wiring: build the layertwine-backed file checkpoint
        // manager (workspace root + scan rules) when enabled and start the
        // manual watcher when a workspace root with manual watching is
        // configured. The manager is attached to the API context so workflow
        // / agent executions create and restore file snapshots through it and
        // script handlers capture workspace changes.
        #[cfg(feature = "checkpoint")]
        let (file_checkpoint_manager, checkpoint_event_bridge_handle) =
            init_file_checkpoint_manager(&config.file_checkpoint, event_bus.clone())?;
        #[cfg(feature = "checkpoint")]
        let manual_change_service =
            init_manual_change_service(&config.file_checkpoint, file_checkpoint_manager.as_ref())?;
        // Approval tool (policy `llm` / `manual`): an LLM node can call
        // `approve_changes` to resolve a pending agent approval in-workflow.
        // Registered only when a file checkpoint manager is attached.
        #[cfg(feature = "checkpoint")]
        if let Some(manager) = &file_checkpoint_manager {
            crate::approval_tool::register_approval_tools(&tool_registry, manager.clone());
        }

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
            sandbox_runtime,
            execution_contexts,
            trigger_listener: Some(listener.listener),
            trigger_listener_shutdown: Some(listener.shutdown),
            trigger_listener_handle: Some(listener.handle),
            trigger_state_registry,
            hook_registry,
            agent_registry,
            #[cfg(feature = "plugins")]
            plugin_engine,
            event_persistence,
            checkpoint_store,
            api_ctx: std::sync::OnceLock::new(),
            #[cfg(feature = "checkpoint")]
            file_checkpoint_manager,
            #[cfg(feature = "checkpoint")]
            manual_change_service,
            #[cfg(feature = "checkpoint")]
            checkpoint_event_bridge_handle,
        })
    }

    pub fn registries(&self) -> &ResourceRegistries {
        &self.registries
    }

    /// The attached file checkpoint manager (layertwine-backed), when file
    /// checkpointing is enabled with a storage backend.
    #[cfg(feature = "checkpoint")]
    pub fn file_checkpoint_manager(&self) -> Option<&wf_checkpoint::file::FileCheckpointManager> {
        self.file_checkpoint_manager.as_ref()
    }

    pub fn bundles(&self) -> &ResourcePluginRegistry {
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

    /// The application-facing API context assembled from the runtime's shared
    /// pieces (storage, registries, event bus, LLM gateway, tool registry).
    ///
    /// Built once and cached: the live execution handles inside the context
    /// (`WorkflowApi` / `AgentApi` pause/resume/cancel) must be shared by all
    /// callers.
    pub fn api_context(&self) -> &wf_api::ApiContext {
        self.api_ctx.get_or_init(|| {
            let storage = self
                .storage_manager
                .shared_context()
                .unwrap_or_else(|| Arc::new(wf_storage::context::StorageContext::new_memory()));
            #[allow(unused_mut)]
            let mut ctx = wf_api::ApiContext::from_runtime_parts(
                storage,
                self.registries.clone(),
                self.bundles.clone(),
                self.event_bus.clone(),
                self.llm_gateway.clone(),
                self.tool_registry.clone(),
                self.metrics.as_ref().map(|m| m.registry().clone()),
            );
            // Plugin contributions (node-type / hook / middleware) are injected
            // into the handler resolution chain (builtin → plugin → template
            // fallback) when the plugin engine is enabled.
            #[cfg(feature = "plugins")]
            if let Some(engine) = &self.plugin_engine {
                ctx = ctx.with_plugin_source(Arc::new(
                    crate::plugin_bridge::WfPluginHandlerSource::new(
                        engine.contribution_manager().clone(),
                    ),
                ));
            }
            // Wire the durable event persistence backend; the event
            // persistence bridge restarts over the new layer.
            if let Some(persistence) = self.event_persistence.clone() {
                ctx = ctx.with_persistence(persistence);
            }
            // Wire the durable checkpoint store so execution checkpoints
            // survive restarts (crash recovery).
            ctx = ctx.with_checkpoint_store(self.checkpoint_store.clone());
            // Share the composite callback's agent loop registry so the
            // server execution views observe tool-dispatched executions.
            ctx = ctx.with_agent_loop_registry(self.agent_registry.clone());
            // Share the trigger runtime state registry so API-created
            // checkpoints capture the trigger audit trail.
            ctx = ctx.with_trigger_state_registry(self.trigger_state_registry.clone());
            // Share the hook receiver registry so API-executed agents and
            // workflows dispatch through the same signal points.
            ctx = ctx.with_hook_registry(self.hook_registry.clone());
            // Attach the file checkpoint manager (file snapshots + script
            // change capture) when file checkpointing is enabled.
            #[cfg(feature = "checkpoint")]
            if let Some(manager) = &self.file_checkpoint_manager {
                ctx = ctx.with_file_checkpoint_manager(manager.clone());
            }
            ctx
        })
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

        // Abort detached execution driver tasks (workflow `stream()` drivers,
        // callback forwarders) before the storage layer closes underneath
        // them.
        if let Some(ctx) = self.api_ctx.get() {
            ctx.shutdown();
        }

        // Flush buffered event persistence before the storage layer closes.
        if let Some(persistence) = self.event_persistence.take() {
            let _ = persistence.shutdown().await;
        }

        // Stop the manual file watcher before the storage layer closes.
        #[cfg(feature = "checkpoint")]
        if let Some(mut service) = self.manual_change_service.take() {
            service.stop().await;
        }

        // Stop the checkpoint event bridge (it holds a broadcast receiver on
        // the checkpoint bus; aborting it is safe once the watcher stopped).
        #[cfg(feature = "checkpoint")]
        if let Some(handle) = self.checkpoint_event_bridge_handle.take() {
            handle.abort();
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

        let stats = self.agent_registry.gate_stats();
        info!(
            "Agent capacity gate final stats: active={}, available={}",
            stats.active_count,
            stats.available_permits
        );

        self.storage_manager.close().await?;
        info!("Runtime shutdown complete");
        Ok(())
    }

    pub fn storage(&self) -> &StorageManager {
        &self.storage_manager
    }

    /// Recover incomplete (running/paused/created) workflow executions left
    /// by a previous process: scans the execution store and restores the
    /// latest checkpoint of each one through the API resume path.
    ///
    /// With the `checkpoint` feature disabled (or without a persistent
    /// checkpoint store) executions are reported as skipped, never as
    /// spuriously recovered.
    pub async fn recover_incomplete_executions(
        &self,
    ) -> RuntimeResult<super::recovery::RecoveryResult> {
        use super::recovery::RecoveryOrchestrator;
        use super::recovery::RecoveryScanner;

        let Some(storage) = self.storage_manager.shared_context() else {
            return Err(crate::error::RuntimeError::NotInitialized);
        };
        let scanner = RecoveryScanner::new(storage.workflow_execution.clone());
        let ctx = self.api_context();

        #[cfg(feature = "checkpoint")]
        {
            return RecoveryOrchestrator::new(scanner)
                .with_recovery_executor(Arc::new(super::recovery::ApiRecoveryExecutor))
                .recover_all(ctx)
                .await;
        }

        #[cfg(not(feature = "checkpoint"))]
        RecoveryOrchestrator::new(scanner).recover_all(ctx).await
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

    /// Shared sandbox runtime (global profiles + routing rules compiled at
    /// bootstrap). Injected into the script handlers of every workflow
    /// execution started by this runtime.
    pub fn sandbox_runtime(&self) -> &Arc<wf_sandbox::SandboxRuntime> {
        &self.sandbox_runtime
    }

    #[cfg(feature = "plugins")]
    pub fn plugin_engine(&self) -> Option<&wf_plugin::PluginEngine> {
        self.plugin_engine.as_ref()
    }
}

/// Resolve the SQLite database path from the runtime storage config, shared
/// by the storage context, the event persistence backend and the checkpoint
/// store so all durable data lands in one file.
fn storage_db_path(config: &StorageConfig) -> PathBuf {
    let app_name = config.app_name.as_deref().unwrap_or("app");
    config
        .sqlite
        .as_ref()
        .map(|c| c.db_path.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("./storage/{}.db", app_name)))
}

/// Build the file checkpoint manager (layertwine-backed) from the
/// file-checkpoint config: storage backend (SQLite by path or in-memory)
/// plus the workspace context (workspace root + scan rules). Returns `None`
/// when file checkpointing is disabled. When enabled, a checkpoint event
/// bridge is started so `CheckpointFileChanged` / `CheckpointMergeConflicted`
/// events (with the `DeltaSummary` payload) flow onto the shared event bus.
#[cfg(feature = "checkpoint")]
fn init_file_checkpoint_manager(
    config: &wf_types::config::file_checkpoint::FileCheckpointConfig,
    event_bus: Arc<wf_core::event::EventBus>,
) -> RuntimeResult<(
    Option<wf_checkpoint::file::FileCheckpointManager>,
    Option<tokio::task::JoinHandle<()>>,
)> {
    if !config.enabled {
        return Ok((None, None));
    }
    match wf_checkpoint::file::FileCheckpointManager::open_from_config(config) {
        Ok(manager) => {
            info!("File checkpoint manager initialized (layertwine SQLite)");
            // The bus is sender-only; the bridge subscribes through it, so
            // recorded file changes and merge conflicts flow onto the shared
            // event bus with their `DeltaSummary` payload.
            let bus = wf_checkpoint::event::CheckpointEventBus::new();
            let handle = crate::checkpoint_event_bridge::spawn(event_bus, bus.clone());
            let manager = manager.with_event_bus(bus);
            Ok((Some(manager), Some(handle)))
        }
        Err(err) => Err(crate::error::RuntimeError::Config(format!(
            "Failed to initialize file checkpoint storage: {err}"
        ))),
    }
}

/// Start the manual change service (watcher -> manual partition) when file
/// checkpointing is enabled with a workspace root and `manual_watch` set.
#[cfg(feature = "checkpoint")]
fn init_manual_change_service(
    config: &wf_types::config::file_checkpoint::FileCheckpointConfig,
    manager: Option<&wf_checkpoint::file::FileCheckpointManager>,
) -> RuntimeResult<Option<wf_checkpoint::watcher::ManualChangeService>> {
    let Some(manager) = manager else {
        return Ok(None);
    };
    let Some(root) = config.workspace_root.as_deref() else {
        return Ok(None);
    };
    if !config.enabled || !config.manual_watch {
        return Ok(None);
    }
    let scan_config = wf_checkpoint::scan::ScanConfig {
        custom_ignore_patterns: config.custom_ignore_patterns.clone().unwrap_or_default(),
        failure_behavior: config.failure_behavior,
    };
    match wf_checkpoint::watcher::ManualChangeService::start(
        manager.clone(),
        root,
        scan_config,
        100,
        200,
    ) {
        Ok(service) => {
            info!(root = %root, "Manual file watcher started");
            Ok(Some(service))
        }
        Err(err) => Err(crate::error::RuntimeError::Config(format!(
            "Failed to start the manual file watcher: {err}"
        ))),
    }
}

/// Build the durable checkpoint store backend mirroring the runtime
/// storage config: SQLite reuses the storage db file, Postgres reuses the
/// connection string, each under its own table. Falls back to in-memory when
/// storage is memory-only or the backend cannot be opened (checkpoints then
/// do not survive restarts).
async fn init_checkpoint_store(config: &StorageConfig) -> Arc<wf_storage::backend::StorageBackend> {
    use wf_storage::backend::StorageBackend;
    use wf_storage::decorator::instrumented::InstrumentedStore;
    use wf_types::config::storage::StorageType;

    let backend = match config.storage_type {
        StorageType::Memory => StorageBackend::new_memory(),
        #[cfg(feature = "sqlite")]
        StorageType::Sqlite => {
            let path = storage_db_path(config);
            match wf_storage::backend::StorageBackend::new_sqlite(
                &path.to_string_lossy(),
                "checkpoint",
            )
            .await
            {
                Ok(store) => store,
                Err(err) => {
                    warn!(error = %err, path = %path.display(), "failed to open checkpoint store backend; checkpoints stay in memory");
                    StorageBackend::new_memory()
                }
            }
        }
        #[cfg(not(feature = "sqlite"))]
        StorageType::Sqlite => {
            warn!("SQLite checkpoint store unavailable: enable the 'sqlite' feature");
            StorageBackend::new_memory()
        }
        #[cfg(feature = "postgres")]
        StorageType::Postgres => {
            let conn = config
                .postgres
                .as_ref()
                .map(|c| c.host.as_str())
                .unwrap_or_default();
            match wf_storage::store::postgres::PostgresStorage::new(conn, "checkpoint").await {
                Ok(store) => StorageBackend::Postgres(InstrumentedStore::new(store)),
                Err(err) => {
                    warn!(error = %err, "failed to open checkpoint store backend; checkpoints stay in memory");
                    StorageBackend::new_memory()
                }
            }
        }
        #[cfg(not(feature = "postgres"))]
        StorageType::Postgres => {
            warn!("PostgreSQL checkpoint store unavailable: enable the 'postgres' feature");
            StorageBackend::new_memory()
        }
    };
    Arc::new(backend)
}

/// Build the durable event persistence backend mirroring the runtime
/// storage config: a buffered layer over a SQLite `StorePersistenceLayer`
/// sharing the storage db file. Returns `None` (events stay in memory) when
/// storage is not SQLite or the backend cannot be opened/initialized.
#[cfg(feature = "sqlite")]
async fn init_event_persistence(
    config: &StorageConfig,
) -> Option<Arc<dyn wf_api::PersistenceLayer>> {
    use wf_types::config::storage::StorageType;

    if config.storage_type != StorageType::Sqlite {
        return None;
    }
    let db_path = storage_db_path(config);

    let layer = match wf_api::StorePersistenceLayer::sqlite(&db_path.to_string_lossy()).await {
        Ok(store) => Arc::new(wf_api::BufferedPersistenceLayer::new(Arc::new(store))),
        Err(err) => {
            warn!(error = %err, path = %db_path.display(), "failed to open event persistence backend; events stay in memory");
            return None;
        }
    };
    if let Err(err) = layer.initialize().await {
        warn!(error = %err, "failed to initialize event persistence backend; events stay in memory");
        return None;
    }
    info!("Event persistence enabled: sqlite at {:?}", db_path);
    Some(layer as Arc<dyn ApiPersistenceLayer>)
}

#[cfg(not(feature = "sqlite"))]
async fn init_event_persistence(_config: &StorageConfig) -> Option<Arc<dyn ApiPersistenceLayer>> {
    None
}

/// Resolve the file-layer infrastructure config into the runtime config.
/// Storage / timeout / metrics / output / sandbox / presets / tools /
/// file_checkpoint are filled only when the programmatic values are still
/// defaults; the skill settings chain is loaded when no programmatic skill
/// config is present; MCP settings chain sources are inherited when absent.
async fn resolve_infra_config(
    mut config: RuntimeConfig,
    infra: &InfraSourceConfig,
) -> RuntimeResult<RuntimeConfig> {
    let project_root = infra.project_root.clone().unwrap_or_default();
    let preset_name = infra
        .preset_name
        .clone()
        .unwrap_or_else(|| wf_config::orchestrator::DEFAULT_INFRA_PRESET.to_string());

    let assembled = ConfigOrchestrator::assemble_with_preset(
        &project_root,
        Some(&preset_name),
        Some(default_infra_file_mapping()),
        Some(infra.overrides.clone()),
    )
    .map_err(|e| {
        crate::error::RuntimeError::Config(format!(
            "Infrastructure config resolution failed (preset `{preset_name}`): {e}"
        ))
    })?;

    if config.storage == StorageConfig::default() {
        config.storage = assembled.storage;
    }
    if config.timeout == TimeoutConfig::default() {
        config.timeout = assembled.timeout;
    }
    if config.output == OutputConfig::default() {
        config.output = assembled.output;
    }
    if config.metrics.is_none() {
        config.metrics = Some(assembled.metrics);
    }
    if config.sandbox.is_none() {
        config.sandbox = assembled.sandbox;
    }
    if config.presets == PresetsConfig::default() {
        config.presets = assembled.presets;
    }
    if config.tools == wf_config::orchestrator::ToolConfigs::default() {
        config.tools = assembled.tools;
    }
    if config.file_checkpoint == FileCheckpointConfig::default() {
        config.file_checkpoint = assembled.file_checkpoint;
    }

    // Skill settings chain (global -> project, or collection mode). Lenient:
    // a missing/invalid skill config falls back to the defaults.
    if config.skills == wf_types::skill::SkillConfig::default() {
        let settings_dir = infra
            .settings_dir
            .as_deref()
            .unwrap_or_else(|| Path::new(""));
        let skills = match &infra.skills_collection {
            Some(name) => wf_config::skill::load_and_merge_skill_config_with_collection(
                settings_dir,
                &project_root,
                Some(name),
            ),
            None => wf_config::skill::load_and_merge_skill_config(settings_dir, &project_root),
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

/// Build the shared LLM gateway: validate and register every configured
/// profile (wf-config llm_profile processors), then attach the runtime token
/// metrics collector when metrics are enabled.
fn init_llm_gateway(
    config: &LlmConfig,
    metrics: Option<&wf_metrics::MetricsRegistry>,
) -> RuntimeResult<Arc<LlmGateway>> {
    let mut gateway = LlmGateway::new();

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
async fn init_mcp(
    config: &McpRuntimeConfig,
) -> Option<Arc<wf_tools::mcp::connection::McpConnectionManager>> {
    use wf_tools::mcp::connection::{McpConnectionManager, McpServerRegistry};

    let (Some(settings_dir), Some(project_root)) = (&config.settings_dir, &config.project_root)
    else {
        return None;
    };

    let settings =
        wf_config::mcp::load_and_merge_mcp_settings(settings_dir, project_root).unwrap_or_default();
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

/// Legacy fallback: register + activate built-in resource plugins through
/// `ResourcePluginRegistry` when the plugin engine is disabled (or the
/// `plugins` feature is off). Keeps goal-review et al. functional without the
/// plugin system.
fn activate_builtin_resource_plugins_legacy(
    bundles: &ResourcePluginRegistry,
    opts: &ResourceOptions,
    registries: &ResourceRegistries,
    tool_registry: &wf_tools::registry::ToolRegistry,
) -> RuntimeResult<()> {
    for plugin in wf_resource::predefined::resource_plugin::builtin_resource_plugins() {
        bundles.register(plugin).map_err(|e| {
            crate::error::RuntimeError::Config(format!(
                "failed to register built-in resource plugin: {e}"
            ))
        })?;
    }
    for sa in &opts.resource_plugin_activation {
        bundles
            .activate(
                &sa.id,
                &sa.config,
                registries,
                tool_registry,
                opts.skip_if_exists,
            )
            .map_err(|e| {
                crate::error::RuntimeError::Config(format!(
                    "failed to activate resource plugin '{}': {e}",
                    sa.id
                ))
            })?;
    }
    Ok(())
}

#[cfg(feature = "plugins")]
async fn init_plugins(
    config: &PluginConfig,
    registries: Arc<wf_resource::registry::ResourceRegistries>,
    tool_registry: Arc<wf_tools::registry::ToolRegistry>,
) -> RuntimeResult<Option<wf_plugin::PluginEngine>> {
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
    let bridge: Option<Arc<dyn wf_plugin::ContributionBridge>> = Some(Arc::new(
        crate::plugin_bridge::WfPluginBridge::new(registries, tool_registry),
    ));

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
    use wf_core::registry::Registry;
    use wf_types::config::storage::StorageType;

    fn clear_env_vars() {
        std::env::remove_var("CLI_MODE");
        std::env::remove_var("HEADLESS");
        std::env::remove_var("TEST_MODE");
        std::env::remove_var("CLI_OUTPUT_FORMAT");
        std::env::remove_var("NO_COLOR");
    }

    /// The checked-in `configs/infrastructure/` bundle (development preset)
    /// fills the runtime config where programmatic values are absent, while
    /// programmatic values keep priority.
    #[test]
    fn test_resolve_infra_config_from_repo_configs() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        if !repo_root.join("configs").join("infrastructure").exists() {
            return;
        }

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let config = runtime.block_on(resolve_infra_config(
            RuntimeConfig::default(),
            &InfraSourceConfig {
                project_root: Some(repo_root.clone()),
                ..Default::default()
            },
        ));
        let config = config.unwrap();

        // File layer fills storage/metrics/timeout/output/sandbox/presets/
        // tools/file_checkpoint from the development preset.
        assert_ne!(config.storage, StorageConfig::default());
        assert!(config.metrics.is_some());
        assert_eq!(config.timeout.default, Some(30000));
        assert_eq!(config.output.dir, "./outputs");
        assert!(config.sandbox.is_some());
        assert!(config.tools.read_file.is_some());
        assert!(
            config.file_checkpoint.custom_ignore_patterns.is_some(),
            "file_checkpoint must load from the file layer"
        );

        // MCP/skills sources are inherited.
        assert_eq!(config.mcp.project_root, Some(repo_root.clone()));

        // Programmatic values win over the file layer.
        let programmatic = RuntimeConfig {
            metrics: Some(MetricsConfig::default()),
            timeout: TimeoutConfig {
                default: Some(11111),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = runtime.block_on(resolve_infra_config(
            programmatic,
            &InfraSourceConfig {
                project_root: Some(repo_root),
                ..Default::default()
            },
        ));
        let config = config.unwrap();
        assert_eq!(config.timeout.default, Some(11111));
        assert!(config.metrics.is_some());
        // storage was left default -> still filled from the file layer.
        assert_ne!(config.storage, StorageConfig::default());
    }

    #[tokio::test]
    async fn test_runtime_bootstrap_memory() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                storage_type: StorageType::Memory,
                sqlite: None,
                postgres: None,
                app_name: None,
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            metrics: None,
            llm: LlmConfig::default(),
            sandbox: None,
            skills: Default::default(),
            mcp: Default::default(),
            shell: Default::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        assert!(runtime.storage().is_initialized());
        assert!(runtime.mode().is_test());
        assert!(!runtime.tool_registry.list().is_empty());

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[test]
    fn test_storage_db_path_resolution() {
        let config = StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: None,
            postgres: None,
            app_name: Some("myapp".into()),
        };
        assert_eq!(
            storage_db_path(&config),
            PathBuf::from("./storage/myapp.db")
        );

        let config = StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: "/data/custom.db".into(),
                ..Default::default()
            }),
            postgres: None,
            app_name: Some("myapp".into()),
        };
        assert_eq!(storage_db_path(&config), PathBuf::from("/data/custom.db"));
    }

    #[tokio::test]
    async fn test_init_checkpoint_store_memory() {
        let config = StorageConfig {
            storage_type: StorageType::Memory,
            sqlite: None,
            postgres: None,
            app_name: None,
        };
        let store = init_checkpoint_store(&config).await;
        assert!(matches!(
            *store,
            wf_storage::backend::StorageBackend::Memory(_)
        ));
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn test_init_checkpoint_store_sqlite_roundtrip() {
        use wf_storage::domain::Store;

        let config = StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: ":memory:".into(),
                ..Default::default()
            }),
            postgres: None,
            app_name: None,
        };
        let store = init_checkpoint_store(&config).await;
        assert!(matches!(
            *store,
            wf_storage::backend::StorageBackend::Sqlite(_)
        ));

        let (data, meta) = (
            b"checkpoint-data".to_vec(),
            serde_json::json!({"entityType": "checkpoint"}),
        );
        store.save("cp-1", &data, &meta).await.unwrap();
        let loaded = store.load("cp-1").await.unwrap().unwrap();
        assert_eq!(loaded.0, data);
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn test_init_checkpoint_store_sqlite_fallback_on_error() {
        let config = StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: "/nonexistent-dir-xyz/foo.db".into(),
                ..Default::default()
            }),
            postgres: None,
            app_name: None,
        };
        let store = init_checkpoint_store(&config).await;
        assert!(matches!(
            *store,
            wf_storage::backend::StorageBackend::Memory(_)
        ));
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
        assert!(!runtime.registries().fragments.is_empty());
        assert!(!runtime.registries().templates.is_empty());
        assert!(!runtime.registries().tool_descriptions.is_empty());
        assert!(!runtime.registries().agent_templates.is_empty());
        assert!(!runtime.registries().workflows.is_empty());

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_api_context_shared_and_cached() {
        clear_env_vars();

        let config = RuntimeConfig {
            log_config: LogConfig::default().with_level("off"),
            resource: ResourceConfig::default(),
            metrics: None,
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        let first: &wf_api::ApiContext = runtime.api_context();
        let second: &wf_api::ApiContext = runtime.api_context();
        assert!(std::ptr::eq(first, second), "api_context must be cached");

        // The context shares the runtime's event bus and tool registry.
        assert!(Arc::ptr_eq(&first.event_bus, &runtime.event_bus));
        assert!(Arc::ptr_eq(&first.tool_registry, &runtime.tool_registry));

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[cfg(feature = "plugins")]
    #[tokio::test]
    async fn test_runtime_goal_review_resource_plugin_activation() {
        clear_env_vars();

        use wf_resource::registry::{RegisterOptions as ResourceOptions, ResourcePluginActivation};
        use wf_workflow::validation::GraphValidator;

        let config = RuntimeConfig {
            log_config: LogConfig::default().with_level("off"),
            resource: ResourceConfig {
                options: ResourceOptions {
                    resource_plugin_activation: vec![ResourcePluginActivation {
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

        // Built-in resource plugin registered and activated through the
        // unified plugin engine: workflow + planner prompt land in the
        // registries via the contribution bridge.
        let engine = runtime
            .plugin_engine()
            .expect("plugin engine is enabled by default");
        assert!(engine.registry().has("@standard/goal-review-agent"));
        assert_eq!(
            engine
                .registry()
                .get("@standard/goal-review-agent")
                .unwrap()
                .status,
            wf_plugin::PluginStatus::Active
        );

        assert!(runtime
            .registries()
            .workflows
            .has("@standard/goal-review-agent-workflow"));
        assert!(runtime
            .registries()
            .templates
            .has("@standard/goal-review-planner"));

        // The assembled workflow is structurally valid (loop pairs, edges,
        // reachability) so it can be executed by the workflow engine.
        let wf = runtime
            .registries()
            .workflows
            .get("@standard/goal-review-agent-workflow")
            .expect("goal review workflow registered");
        let graph = crate::trigger_listener::template_to_graph(&wf);
        GraphValidator::validate(graph).unwrap_or_else(|errors| {
            panic!(
                "goal review workflow failed validation: {:?}",
                errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
            )
        });

        // Deactivation via the plugin engine removes the workflow and prompt.
        engine.deactivate("@standard/goal-review-agent").await.unwrap();
        assert_eq!(
            engine
                .registry()
                .get("@standard/goal-review-agent")
                .unwrap()
                .status,
            wf_plugin::PluginStatus::Deactivated
        );
        assert!(!runtime
            .registries()
            .workflows
            .has("@standard/goal-review-agent-workflow"));
        assert!(!runtime
            .registries()
            .templates
            .has("@standard/goal-review-planner"));

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_sandbox_config_compiled_at_bootstrap() {
        clear_env_vars();

        use wf_types::script::sandbox::{
            SandboxConfig, SandboxGlobalConfig, SandboxMode, SandboxProfile, SandboxProfileRule,
            SandboxRuleMatchField,
        };

        // Valid global config: bootstrap succeeds and exposes the compiled
        // shared sandbox runtime; the rule routes shell executions to the
        // Lenient profile.
        let global = SandboxGlobalConfig {
            mode: Some(SandboxMode::Strict),
            profiles: vec![SandboxProfile {
                name: "lenient".to_string(),
                description: None,
                mode: Some(SandboxMode::Lenient),
                shell_strategy: None,
                python_strategy: None,
                javascript_strategy: None,
                lua_strategy: None,
                policy: None,
                vfs: None,
                workdir: None,
                env: None,
            }],
            rules: vec![SandboxProfileRule {
                match_field: SandboxRuleMatchField::Language,
                match_pattern: "shell".to_string(),
                profile: "lenient".to_string(),
            }],
            default_profile: None,
            audit_logging: true,
        };
        let config = RuntimeConfig {
            sandbox: Some(global),
            ..Default::default()
        };
        let runtime = Runtime::bootstrap(config).await.unwrap();
        let result = runtime
            .sandbox_runtime()
            .execute(
                "shell",
                "echo hello",
                &SandboxConfig {
                    mode: None,
                    policy: None,
                    shell_strategy: None,
                    python_strategy: None,
                    javascript_strategy: None,
                    lua_strategy: None,
                    vfs: None,
                    workdir: None,
                    env: None,
                    legacy_type: None,
                    resource_limits: None,
                    skip_gate_check: None,
                },
            )
            .await;
        assert!(
            result.success,
            "shared sandbox runtime must execute shell: {:?}",
            result.error
        );
        assert_eq!(
            result.sandbox_mode,
            Some("Lenient".to_string()),
            "rule must route shell to the lenient profile"
        );
        runtime.shutdown().await.unwrap();

        // Invalid config (rule references unknown profile): bootstrap must
        // fail fast instead of deferring the error to script execution.
        let bad = SandboxGlobalConfig {
            rules: vec![SandboxProfileRule {
                match_field: SandboxRuleMatchField::Language,
                match_pattern: "shell".to_string(),
                profile: "does-not-exist".to_string(),
            }],
            ..Default::default()
        };
        let config = RuntimeConfig {
            sandbox: Some(bad),
            ..Default::default()
        };
        let err = match Runtime::bootstrap(config).await {
            Ok(_) => panic!("invalid sandbox global config must fail bootstrap"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("Invalid sandbox global config"),
            "error: {err}"
        );
        assert!(err.to_string().contains("unknown profile"), "error: {err}");

        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_trigger_shutdown() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                storage_type: StorageType::Memory,
                sqlite: None,
                postgres: None,
                app_name: None,
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            metrics: None,
            llm: LlmConfig::default(),
            sandbox: None,
            skills: Default::default(),
            mcp: Default::default(),
            shell: Default::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
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
        assert!(matches!(config.storage.storage_type, StorageType::Memory));
        assert!(config.mode_override.is_none());
        assert!(config.metrics.is_none());
    }

    #[tokio::test]
    async fn test_runtime_metrics_wiring() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                storage_type: StorageType::Memory,
                sqlite: None,
                postgres: None,
                app_name: None,
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            skills: Default::default(),
            mcp: Default::default(),
            shell: Default::default(),
            metrics: Some(wf_types::config::metrics::MetricsConfig {
                workflow_metrics: Some(wf_types::config::metrics::MetricCollectorConfig {
                    flush_interval: Some(100),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            llm: LlmConfig::default(),
            sandbox: None,
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        let metrics = runtime
            .metrics()
            .expect("metrics system should be initialized");
        metrics.registry().workflow().record_execution_start("wf-1");
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
                storage_type: StorageType::Memory,
                sqlite: None,
                postgres: None,
                app_name: None,
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            skills: Default::default(),
            mcp: Default::default(),
            shell: Default::default(),
            metrics: Some(wf_types::config::metrics::MetricsConfig {
                enabled: Some(false),
                ..Default::default()
            }),
            llm: LlmConfig::default(),
            sandbox: None,
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();
        assert!(runtime.metrics().is_none());

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    /// Query an execution through the query_workflow_status tool, exercising
    /// the registry-bound composite callback exactly like production.
    async fn query_status_via_tool(runtime: &Runtime, execution_id: &str) -> serde_json::Value {
        let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new("callback-test".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };
        let result = runtime
            .tool_registry()
            .execute_tool(
                "query_workflow_status",
                &serde_json::json!({ "workflow_id": execution_id, "execution_id": execution_id }),
                &options,
                &ctx,
            )
            .await
            .expect("query tool must succeed");
        assert!(result.success, "query failed: {:?}", result.error);
        result.result.unwrap()
    }

    #[tokio::test]
    async fn test_execution_callback_wired_call_agent_via_tool() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                storage_type: StorageType::Memory,
                sqlite: None,
                postgres: None,
                app_name: None,
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            metrics: None,
            llm: LlmConfig::default(),
            sandbox: None,
            skills: Default::default(),
            mcp: Default::default(),
            shell: Default::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let runtime = Runtime::bootstrap(config).await.unwrap();

        // Builtin tool definitions are registered by wf-resource; register
        // the dispatch defs used by this test into the shared registry.
        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::agent::CALL_AGENT.tool_def());
        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::workflow::QUERY_WORKFLOW_STATUS.tool_def());

        let mock = Arc::new(wf_llm::mock::MockLlmClient::new());
        mock.default(wf_llm::mock::LlmResponseSpec::text("agent answer"));
        runtime.llm_gateway().register_mock("mock", mock);

        let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new("callback-test".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        // call_agent through the shared tool registry hits the composite
        // callback (previously CallbackNotRegistered in production).
        let result = runtime
            .tool_registry()
            .execute_tool(
                "call_agent",
                &serde_json::json!({
                    "agent_id": "integration-agent",
                    "agent_profile_id": "mock",
                    "prompt": "hello",
                }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success, "call_agent failed: {:?}", result.error);
        assert_eq!(result.result.unwrap()["result"], "agent answer");

        // The execution is registered in the shared agent loop registry and
        // observable through the query_workflow_status tool with its result.
        let ids = runtime.api_context().agent_loops.get_all_ids();
        assert_eq!(ids.len(), 1, "execution must be registered");
        let status = query_status_via_tool(&runtime, &ids[0].to_string()).await;
        assert_eq!(status["status"], "completed");
        assert_eq!(status["result"], "agent answer");

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[tokio::test]
    async fn test_execution_callback_call_agent_wait_false_spawns() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                storage_type: StorageType::Memory,
                sqlite: None,
                postgres: None,
                app_name: None,
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            metrics: None,
            llm: LlmConfig::default(),
            sandbox: None,
            skills: Default::default(),
            mcp: Default::default(),
            shell: Default::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let runtime = Runtime::bootstrap(config).await.unwrap();

        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::agent::CALL_AGENT.tool_def());
        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::workflow::QUERY_WORKFLOW_STATUS.tool_def());

        let mock = Arc::new(wf_llm::mock::MockLlmClient::new());
        mock.default(wf_llm::mock::LlmResponseSpec::text("async answer"));
        runtime.llm_gateway().register_mock("mock", mock);

        let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new("callback-test".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let result = runtime
            .tool_registry()
            .execute_tool(
                "call_agent",
                &serde_json::json!({
                    "agent_id": "integration-agent",
                    "agent_profile_id": "mock",
                    "prompt": "hello",
                    "wait": false,
                }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(
            result.success,
            "spawned call_agent failed: {:?}",
            result.error
        );
        let value = result.result.unwrap();
        assert_eq!(value["status"], "started");
        let execution_id = value["execution_id"].as_str().unwrap().to_string();

        // The spawned execution progresses in the background; polling the
        // query tool eventually returns the result.
        let mut final_result = None;
        for _ in 0..200 {
            let status = query_status_via_tool(&runtime, &execution_id).await;
            if status["status"] == "completed" {
                final_result = Some(status["result"].clone());
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(final_result, Some(serde_json::json!("async answer")));

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[tokio::test]
    async fn test_execution_callback_execute_workflow_via_tool() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                storage_type: StorageType::Memory,
                sqlite: None,
                postgres: None,
                app_name: None,
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            metrics: None,
            llm: LlmConfig::default(),
            sandbox: None,
            skills: Default::default(),
            mcp: Default::default(),
            shell: Default::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let runtime = Runtime::bootstrap(config).await.unwrap();

        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::workflow::EXECUTE_WORKFLOW.tool_def());
        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::workflow::QUERY_WORKFLOW_STATUS.tool_def());

        // The llm_summary_workflow LLM node uses the DEFAULT profile.
        let mock = Arc::new(wf_llm::mock::MockLlmClient::new());
        mock.default(wf_llm::mock::LlmResponseSpec::text("compressed").with_usage(50, 30));
        runtime.llm_gateway().register_mock("DEFAULT", mock);

        let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new("callback-test".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let message = wf_types::message::Message {
            id: wf_common::generate_id(),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text("long context".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        let result = runtime
            .tool_registry()
            .execute_tool(
                "execute_workflow",
                &serde_json::json!({
                    "workflow_id": "llm_summary_workflow",
                    "input": {
                        "conversationHistory": [
                            serde_json::to_value(&message).unwrap()
                        ]
                    }
                }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(
            result.success,
            "execute_workflow failed: {:?}",
            result.error
        );
        let value = result.result.unwrap();
        let execution_id = value["execution_id"].as_str().unwrap().to_string();

        // Registered resource workflows are resolvable; the execution
        // completes and its status is queryable through the query tool.
        let mut terminal = false;
        for _ in 0..200 {
            let status = query_status_via_tool(&runtime, &execution_id).await;
            let state = status["status"].as_str().unwrap_or_default().to_string();
            if state == "completed" || state == "failed" {
                terminal = true;
                assert_eq!(state, "completed", "status: {status:?}");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(terminal, "workflow execution did not settle");

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
                storage_type: StorageType::Memory,
                sqlite: None,
                postgres: None,
                app_name: None,
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
            sandbox: None,
            shell: Default::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        let manager = runtime.mcp_manager().expect("MCP manager initialized");
        assert_eq!(manager.registry().list().len(), 1);
        assert!(
            manager.connected_servers().is_empty(),
            "lazy server not connected"
        );

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
            context_window_size: None,
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
                    context_window_size: None,
                }],
            },
            None,
        ) {
            Ok(_) => panic!("invalid profile must be rejected"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("LLM profile"));
    }

    #[tokio::test]
    async fn test_shell_events_bridged_to_event_bus() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                storage_type: StorageType::Memory,
                sqlite: None,
                postgres: None,
                app_name: None,
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            metrics: None,
            llm: LlmConfig::default(),
            sandbox: None,
            skills: Default::default(),
            mcp: Default::default(),
            shell: wf_shell::config::ShellToolConfig {
                output_event_enabled: true,
                ..Default::default()
            },
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();
        let mut sub = runtime.event_bus.subscribe();

        // Tool definitions are registered by wf-resource in production;
        // register the shell defs used by this test directly.
        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::shell::GET_OR_CREATE_SHELL.tool_def());

        std::fs::create_dir_all("/tmp/bootstrap-shell-events").unwrap();
        let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new("exec-bridge".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };
        let result = runtime
            .tool_registry()
            .execute_tool(
                "get_or_create_shell",
                &serde_json::json!({ "cwd": "/tmp/bootstrap-shell-events" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success);

        // The created event is delivered on a background dispatch thread;
        // poll until it arrives.
        let mut saw_created = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while std::time::Instant::now() < deadline {
            match sub.try_recv() {
                Ok(event) => {
                    if event.r#type == wf_types::events::EventType::ShellSessionCreated {
                        saw_created = true;
                        assert_eq!(
                            event.metadata.unwrap()["session_id"],
                            result.result.unwrap()["session_id"]
                        );
                        break;
                    }
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        assert!(
            saw_created,
            "no ShellSessionCreated event on the runtime EventBus"
        );

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }
}
