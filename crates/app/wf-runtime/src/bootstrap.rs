#[path = "bootstrap_config.rs"]
mod bootstrap_config;
#[path = "bootstrap_helpers.rs"]
mod bootstrap_helpers;

use std::sync::Arc;

use tracing::{info, warn};
use wf_workflow::trigger_listener::TriggerEventListener;

use wf_core::event::EventBus;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::registry::{MutableRegistry, Registry};
use wf_execution_shared::hooks::HookRegistry;
use wf_llm::LlmGateway;
use wf_resource::registry::ResourceRegistries;
use wf_resource::resource_plugin::ResourcePluginRegistry;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::error::RuntimeResult;
use crate::lifecycle::{shutdown_channel, ShutdownHandle, ShutdownWaiter};
use crate::logger::init_tracing;
use crate::metrics::MetricsContext;
use crate::mode::{detect_all, ModeInfo};
use crate::storage_manager::StorageManager;
use crate::trigger_listener::{
    register_compression_receiver, start_trigger_listener_with_parts, ExecutionContextRegistry,
    TriggerExecutionRecorder, WorkflowRunner,
};

#[cfg(feature = "plugins")]
pub use bootstrap_config::PluginConfig;
pub use bootstrap_config::{
    InfraSourceConfig, LlmConfig, McpRuntimeConfig, ResourceConfig, RuntimeConfig,
};
pub use bootstrap_helpers::activate_builtin_resource_plugins_legacy;
#[cfg(feature = "plugins")]
pub use bootstrap_helpers::init_plugins;
pub use bootstrap_helpers::{
    adjust_log_config, hydrate_tool_registry_from_storage, init_checkpoint_store,
    init_event_persistence, init_llm_gateway, init_mcp, init_metrics_context,
    init_plugins_and_resources, init_tool_registry_with_mcp, resolve_infra_config, storage_db_path,
};
#[cfg(feature = "checkpoint")]
pub use bootstrap_helpers::{
    init_file_checkpoint_manager, init_gc_timer, init_manual_change_service,
};

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
    api_ctx: std::sync::OnceLock<std::sync::Arc<wf_api::ApiContext>>,
    /// File checkpoint manager (layertwine-backed): execution file snapshots
    /// are created/restored through it and script handlers capture workspace
    /// changes when it is attached. `None` keeps file checkpointing disabled.
    #[cfg(feature = "checkpoint")]
    file_checkpoint_manager: Option<wf_checkpoint::file::FileCheckpointManager>,
    /// Host default tool approval configuration, applied to the API context
    /// so executions launched through it route tool calls through the
    /// persisted interaction flow when enabled.
    tool_approval: wf_types::config::tool_approval::ToolApprovalConfig,
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
    /// Optional periodic GC timer that runs `FileCheckpointManager::run_gc`
    /// at the configured `gc_interval_secs` interval. `None` when periodic
    /// GC is disabled (explicit `run_gc` / API only).
    #[cfg(feature = "checkpoint")]
    gc_timer_handle: Option<tokio::task::JoinHandle<()>>,
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
    signal_bus: Arc<InternalSignalBus>,
    llm_gateway: Arc<LlmGateway>,
    tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    sandbox_runtime: Arc<wf_sandbox::SandboxRuntime>,
    agent_executor: Arc<wf_agent::executor::AgentLoopExecutor>,
    hook_registry: Arc<HookRegistry>,
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    limits: wf_types::config::limits::LimitsConfig,
) -> TriggerSubsystem {
    let execution_contexts = Arc::new(ExecutionContextRegistry::new());
    let trigger_state_registry = Arc::new(wf_workflow::TriggerStateRegistry::new());
    let trigger_shutdown = tokio_util::sync::CancellationToken::new();
    let subworkflow_runner: std::sync::Arc<dyn wf_workflow::trigger_listener::SubworkflowRunner> =
        std::sync::Arc::new(
            WorkflowRunner::with_tool_registry(
                registries.clone(),
                event_bus.clone(),
                llm_gateway.clone(),
                execution_contexts.clone(),
                Some(tool_registry.clone()),
                Some(sandbox_runtime.clone()),
            )
            .with_signal_bus(signal_bus.clone())
            .with_limits(limits),
        );
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
        Some(signal_bus.clone()),
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
        // Shared typed signal bus: internal workflow/agent control signals
        // (stop/pause/resume/skip, async results) replace the `__`-prefixed
        // variable protocol. Created beside the event bus so trigger actions
        // and coordinators share one instance.
        let signal_bus = Arc::new(InternalSignalBus::new());

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

        let mut shell_config = config.shell.clone();
        let tool_registry = init_tool_registry_with_mcp(
            &mut shell_config,
            &sandbox_runtime,
            skill_loader.clone(),
            &mcp_manager,
            &event_bus,
        )
        .await?;

        #[cfg(feature = "plugins")]
        let plugin_engine =
            init_plugins(&config.plugins, registries.clone(), tool_registry.clone()).await?;
        init_plugins_and_resources(
            &bundles,
            &config.resource.options,
            &registries,
            &tool_registry,
            #[cfg(feature = "plugins")]
            &plugin_engine,
        )
        .await?;

        // Hydrate persisted agent templates into the runtime registry so
        // templates created through the API survive restarts. Predefined
        // and plugin-owned entries keep their registry version.
        if let Some(storage_ctx) = storage_manager.shared_context() {
            if let Ok(templates) = storage_ctx.agent_template.list(None).await {
                let mut restored = 0usize;
                for template in templates {
                    let key = template.id.to_string();
                    if !registries.agent_templates.has(&key) {
                        if let Err(e) = registries
                            .agent_templates
                            .register(key, std::sync::Arc::new(template))
                        {
                            warn!(error = %e, "hydrate persisted agent template skipped");
                        } else {
                            restored += 1;
                        }
                    }
                }
                if restored > 0 {
                    info!(
                        "Restored {} persisted agent template(s) from storage",
                        restored
                    );
                }
            }
        }

        // The agent loop registry is created before metrics so the runtime
        // can wire its capacity gate into the resource sampler from the
        // first observation tick.
        let agent_registry = std::sync::Arc::new(wf_agent::registry::AgentLoopRegistry::new());
        let gate_stats = agent_registry.gate_stats();
        info!(
            "Agent capacity gate at startup: max_concurrent={}, active={}, available={}",
            gate_stats.max_concurrent, gate_stats.active_count, gate_stats.available_permits
        );

        let metrics = init_metrics_context(
            &config.metrics,
            &storage_manager,
            &event_bus,
            &agent_registry,
        )
        .await?;

        let (shutdown_handle, _shutdown_waiter) = shutdown_channel();

        let llm_gateway =
            init_llm_gateway(&config.llm, metrics.as_ref().map(|m| m.registry().as_ref()))?;

        // Execution callback assembly: a composite covering agent and
        // workflow dispatch, registered on both the global callback
        // singleton and the shared tool registry. Fixes the production path
        // where builtin dispatch tools previously failed with
        // CallbackNotRegistered.
        let agent_limits = config.limits.agent.clone().unwrap_or_default();
        let agent_executor = std::sync::Arc::new(
            wf_agent::executor::AgentLoopExecutor::new(llm_gateway.clone(), tool_registry.clone())
                .with_shared_registry(agent_registry.clone())
                .with_max_iterations_cap(
                    agent_limits
                        .max_iterations_cap
                        .unwrap_or(wf_agent::constants::AGENT_MAX_ITERATIONS_CAP),
                )
                .with_max_iterations(
                    agent_limits
                        .default_max_iterations
                        .unwrap_or(wf_agent::constants::DEFAULT_MAX_ITERATIONS),
                )
                .with_max_sub_agent_depth(
                    agent_limits
                        .max_sub_agent_depth
                        .unwrap_or(wf_agent::registry::DEFAULT_MAX_SUB_AGENT_DEPTH),
                )
                .with_max_concurrent({
                    let max = agent_limits.max_concurrent.unwrap_or(0);
                    if max == 0 {
                        std::thread::available_parallelism()
                            .map(|n| n.get())
                            .unwrap_or(4)
                    } else {
                        max as usize
                    }
                })
                .with_hook_registry(hook_registry.clone())
                .with_signal_bus(signal_bus.clone()),
        );
        let mut workflow_callback =
            wf_workflow::execution_callback::WorkflowExecutionCallback::new(tool_registry.clone())
                .with_gateway(llm_gateway.clone())
                .with_event_bus(event_bus.clone())
                .with_sandbox(sandbox_runtime.clone())
                .with_hook_registry(hook_registry.clone())
                .with_signal_bus(signal_bus.clone());
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

        hydrate_tool_registry_from_storage(&tool_registry, &storage_manager).await;

        // Event-driven trigger subsystem: powers the nested-agent-execution
        // action (HookTriggered etc.) and user trigger templates. The context
        // compression chain is now served by the hook registry: the engine
        // dispatches the CONTEXT_COMPRESSION_REQUESTED signal synchronously
        // and the compression receiver (assembled below) takes over
        // immediately.
        let trigger_subsystem = assemble_trigger_subsystem(
            registries.clone(),
            event_bus.clone(),
            signal_bus.clone(),
            llm_gateway.clone(),
            tool_registry.clone(),
            sandbox_runtime.clone(),
            agent_executor.clone(),
            hook_registry.clone(),
            storage_manager.shared_context().map(|ctx| {
                Arc::new(ctx.trigger_execution.clone()) as Arc<dyn TriggerExecutionRecorder>
            }),
            config.limits.clone(),
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

        // Optional periodic GC timer: when `gc_interval_secs` is configured,
        // spawn a background task that runs `run_gc` at the specified interval.
        #[cfg(feature = "checkpoint")]
        let gc_timer_handle =
            init_gc_timer(&config.file_checkpoint, file_checkpoint_manager.as_ref());

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
            tool_approval: config.tool_approval.clone(),
            #[cfg(feature = "checkpoint")]
            manual_change_service,
            #[cfg(feature = "checkpoint")]
            checkpoint_event_bridge_handle,
            #[cfg(feature = "checkpoint")]
            gc_timer_handle,
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
    fn ensure_api_context(&self) -> &std::sync::Arc<wf_api::ApiContext> {
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
            // Apply the host default tool approval config: when enabled,
            // executions launched through this context route every tool call
            // through the persisted interaction flow (the library default
            // without a handler stays auto-approve).
            ctx = ctx.with_tool_approval(self.tool_approval.clone());
            std::sync::Arc::new(ctx)
        })
    }

    pub fn api_context(&self) -> &wf_api::ApiContext {
        self.ensure_api_context().as_ref()
    }

    pub fn api_context_arc(&self) -> std::sync::Arc<wf_api::ApiContext> {
        std::sync::Arc::clone(self.ensure_api_context())
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

        // Stop the periodic GC timer before the storage layer closes.
        #[cfg(feature = "checkpoint")]
        if let Some(handle) = self.gc_timer_handle.take() {
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
            stats.active_count, stats.available_permits
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

#[path = "bootstrap_tests.rs"]
#[cfg(test)]
mod tests;
