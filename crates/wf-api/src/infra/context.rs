use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use wf_agent::registry::AgentLoopRegistry;
use wf_core::registry::{ConcurrentRegistry, Registry};
use wf_core::EventBus;
use wf_execution_shared::execution_state::ExecutionStateManager;
use wf_execution_shared::hooks::HookRegistry;
use wf_llm::LlmGateway;
use wf_metrics::MetricsRegistry;
use wf_resource::registry::ResourceRegistries;
use wf_resource::resource_plugin::ResourcePluginRegistry;
use wf_storage::backend::StorageBackend;
use wf_storage::context::StorageContext;
use wf_tools::registry::ToolRegistry;
use wf_types::enums::MiddlewarePhase;
use wf_types::node::StaticNodeType;
use wf_workflow::entity::WorkflowExecutionEntity;
use wf_workflow::handler::NodeHandler;
use wf_workflow::registry::WorkflowExecutionRegistry;

use crate::infra::handler_chain::{node_type_name, NoopPluginHandlerSource, PluginHandlerSource};
use crate::infra::persistence::{PersistenceLayer, StorePersistenceLayer};
use crate::infra::tasks::ExecutionTaskRegistry;
use crate::ApiResult;

/// Assembled application-facing API context.
///
/// Composes the storage layer (persistent source) with the execution engines
/// (`wf-workflow` / `wf-agent`) and the shared runtime pieces the engines
/// need. Constructed by `wf-runtime` (or an app) through the builder methods;
/// `wf-api` itself never depends on `wf-runtime`.
pub struct ApiContext {
    pub storage: Arc<StorageContext>,
    pub registries: Arc<ResourceRegistries>,
    pub bundles: Arc<ResourcePluginRegistry>,
    /// Shared event bus; workflow/agent engines publish lifecycle events here
    /// and `ExecutionEventStream` subscribes to them.
    pub event_bus: Arc<EventBus>,
    pub metrics: Option<Arc<MetricsRegistry>>,
    pub llm_gateway: Arc<LlmGateway>,
    pub tool_registry: Arc<ToolRegistry>,
    /// Shared sandbox runtime used by `ScriptApi::execute` and the script
    /// handlers. Created once per context; profiles/rules are compiled at
    /// construction so per-execution setup stays cheap.
    pub sandbox: Arc<wf_sandbox::SandboxRuntime>,
    /// Storage backend used by the checkpoint integrations of executions
    /// launched through this context (defaults to in-memory).
    pub checkpoint_store: Arc<StorageBackend>,
    /// Live workflow execution handles (pause/resume/cancel/status queries).
    pub workflow_executions: WorkflowExecutionRegistry,
    /// Live agent loop execution handles (pause/resume/cancel/status queries).
    pub agent_loops: Arc<AgentLoopRegistry>,
    /// Template usage counters (workflow/agent template library), keyed by
    /// template id. In-memory analytics; not persisted.
    pub template_usage: Arc<DashMap<String, u64>>,
    /// Node handlers shared by every workflow execution.
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    /// Shared user interaction handler slot (agent config approval / follow-up
    /// wiring). Read and written by `AgentUserInteractionApi`.
    pub user_interaction_handler: Arc<
        tokio::sync::RwLock<
            Option<Arc<dyn crate::agent::agent_user_interaction::UserInteractionHandler>>,
        >,
    >,
    /// Unified write point for persisted execution records
    /// (`WorkflowExecution` / `AgentExecution`). Wired to the storage
    /// adapters; the engines persist through it and `wf-api` stays read-only.
    pub state_manager: ExecutionStateManager,
    /// Durable event / snapshot / metric persistence (buffered + backend).
    /// History/timeline/stats queries in `EventApi` read through this layer.
    pub persistence: Arc<dyn PersistenceLayer>,
    /// Plugin contribution source (node executors / hooks / middleware)
    /// injected by `wf-runtime`. `wf-api` stays independent of `wf-plugin` by
    /// consuming contributions through this trait.
    pub plugin_source: Arc<dyn PluginHandlerSource>,
    /// Abort handles of detached execution driver tasks (workflow `stream()`
    /// drivers, callback forwarders). Teardown (`shutdown`) hard-cancels
    /// anything still running.
    pub execution_tasks: Arc<ExecutionTaskRegistry>,
    /// Background task persisting bus events through `persistence`. Restarted
    /// when `with_persistence` swaps the layer.
    persistence_bridge: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Trigger runtime state of live workflow executions (which event-driven
    /// triggers fired). Captured into checkpoint `trigger_states` for audit.
    pub trigger_state_registry: Arc<wf_workflow::TriggerStateRegistry>,
    /// Shared hook receiver registry: hook points and engine signals of
    /// executions launched through this context dispatch through it.
    pub hook_registry: Option<Arc<HookRegistry>>,
    /// Optional file checkpoint manager (layertwine-backed): file snapshots
    /// of executions are created/restored through it, and the script handlers
    /// capture workspace changes when it is attached. `None` keeps file
    /// checkpointing disabled.
    file_checkpoint_manager: Option<wf_checkpoint::file::FileCheckpointManager>,
}

impl ApiContext {
    pub fn new(
        storage: StorageContext,
        registries: Arc<ResourceRegistries>,
        bundles: Arc<ResourcePluginRegistry>,
    ) -> Self {
        let storage = Arc::new(storage);
        let event_bus = Arc::new(EventBus::new(1024));
        let llm_gateway = Arc::new(LlmGateway::new());
        let handlers = wf_workflow::create_default_handlers(llm_gateway.clone(), None);
        let ctx = Self {
            storage: storage.clone(),
            registries,
            bundles,
            event_bus,
            metrics: None,
            llm_gateway,
            tool_registry: Arc::new(ToolRegistry::new()),
            sandbox: Arc::new(wf_sandbox::SandboxRuntime::new()),
            checkpoint_store: Arc::new(StorageBackend::new_memory()),
            workflow_executions: ConcurrentRegistry::new(),
            agent_loops: Arc::new(AgentLoopRegistry::new()),
            template_usage: Arc::new(DashMap::new()),
            user_interaction_handler: Arc::new(tokio::sync::RwLock::new(None)),
            state_manager: ExecutionStateManager::new()
                .with_workflow_store(Arc::new(storage.workflow_execution.clone()))
                .with_agent_store(Arc::new(storage.agent_execution.clone())),
            persistence: Arc::new(StorePersistenceLayer::memory()),
            plugin_source: Arc::new(NoopPluginHandlerSource),
            execution_tasks: Arc::new(ExecutionTaskRegistry::new()),
            persistence_bridge: std::sync::Mutex::new(None),
            handlers,
            trigger_state_registry: Arc::new(wf_workflow::TriggerStateRegistry::new()),
            hook_registry: None,
            file_checkpoint_manager: None,
        };
        // Persist every engine event published on the shared bus.
        ctx.restart_persistence_bridge();
        ctx
    }

    /// Convenience constructor wiring the pieces a runtime bootstrap already
    /// owns: the shared storage context, resource registries, the shared event
    /// bus, the LLM gateway and the shared tool registry.
    pub fn from_runtime_parts(
        storage: Arc<StorageContext>,
        registries: Arc<ResourceRegistries>,
        bundles: Arc<ResourcePluginRegistry>,
        event_bus: Arc<EventBus>,
        llm_gateway: Arc<LlmGateway>,
        tool_registry: Arc<ToolRegistry>,
        metrics: Option<Arc<MetricsRegistry>>,
    ) -> Self {
        let handlers = wf_workflow::create_default_handlers(llm_gateway.clone(), None);
        let ctx = Self {
            storage: storage.clone(),
            registries,
            bundles,
            event_bus,
            metrics,
            llm_gateway,
            tool_registry,
            sandbox: Arc::new(wf_sandbox::SandboxRuntime::new()),
            checkpoint_store: Arc::new(StorageBackend::new_memory()),
            workflow_executions: ConcurrentRegistry::new(),
            agent_loops: Arc::new(AgentLoopRegistry::new()),
            template_usage: Arc::new(DashMap::new()),
            user_interaction_handler: Arc::new(tokio::sync::RwLock::new(None)),
            state_manager: ExecutionStateManager::new()
                .with_workflow_store(Arc::new(storage.workflow_execution.clone()))
                .with_agent_store(Arc::new(storage.agent_execution.clone())),
            persistence: Arc::new(StorePersistenceLayer::memory()),
            plugin_source: Arc::new(NoopPluginHandlerSource),
            execution_tasks: Arc::new(ExecutionTaskRegistry::new()),
            persistence_bridge: std::sync::Mutex::new(None),
            handlers,
            trigger_state_registry: Arc::new(wf_workflow::TriggerStateRegistry::new()),
            hook_registry: None,
            file_checkpoint_manager: None,
        };
        // Persist every engine event published on the shared bus.
        ctx.restart_persistence_bridge();
        ctx
    }

    /// Inject the shared hook receiver registry (hook points + engine
    /// signals of executions launched through this context).
    pub fn with_hook_registry(mut self, registry: Arc<HookRegistry>) -> Self {
        self.hook_registry = Some(registry);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn with_checkpoint_store(mut self, store: Arc<StorageBackend>) -> Self {
        self.checkpoint_store = store;
        self
    }

    pub fn with_handlers(
        mut self,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    ) -> Self {
        self.handlers = handlers;
        self
    }

    /// Swap in a custom persistence layer (e.g. buffered + sqlite from the
    /// runtime bootstrap) and restart the event persistence bridge so
    /// subsequent bus events land in the new layer.
    pub fn with_persistence(mut self, persistence: Arc<dyn PersistenceLayer>) -> Self {
        self.persistence = persistence;
        self.restart_persistence_bridge();
        self
    }

    /// Inject the plugin contribution source (built by `wf-runtime` over the
    /// plugin engine's `ContributionManager`).
    pub fn with_plugin_source(mut self, plugin_source: Arc<dyn PluginHandlerSource>) -> Self {
        self.plugin_source = plugin_source;
        self
    }

    /// Inject a shared agent loop registry so server-started executions and
    /// tool-dispatched executions (through the runtime composite callback)
    /// share one view. Without injection a private registry is used.
    pub fn with_agent_loop_registry(mut self, registry: Arc<AgentLoopRegistry>) -> Self {
        self.agent_loops = registry;
        self
    }

    /// Inject the shared trigger runtime state registry (wf-runtime listener
    /// records into it; checkpoints capture its `trigger_states` audit trail).
    pub fn with_trigger_state_registry(
        mut self,
        registry: Arc<wf_workflow::TriggerStateRegistry>,
    ) -> Self {
        self.trigger_state_registry = registry;
        self
    }

    /// Attach the file checkpoint manager. When attached:
    /// - workflow/agent checkpoint coordinators create/restore file
    ///   snapshots alongside execution checkpoints,
    /// - the shared handler set is rebuilt so script handlers capture their
    ///   workspace changes onto the executing actor partition.
    pub fn with_file_checkpoint_manager(
        mut self,
        manager: wf_checkpoint::file::FileCheckpointManager,
    ) -> Self {
        self.file_checkpoint_manager = Some(manager.clone());
        let handlers = wf_workflow::create_default_handlers_with_file_checkpoint(
            self.llm_gateway.clone(),
            Some(self.sandbox.clone()),
            Some(manager),
        );
        self.handlers = handlers;
        self
    }

    /// The attached file checkpoint manager, if any.
    pub fn file_checkpoint_manager(&self) -> Option<&wf_checkpoint::file::FileCheckpointManager> {
        self.file_checkpoint_manager.as_ref()
    }

    pub fn handlers(&self) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
        self.handlers.clone()
    }

    /// Resolve the builtin handler for `node_type` from the shared handler
    /// map. The map is immutable after construction, so callers borrow the
    /// resolved handler rather than taking ownership of a per-handler clone.
    ///
    /// Plugin contributions (executors / middleware) are resolved
    /// through `plugin_source` instead (see
    /// [`Self::has_plugin_node_executor`]).
    pub fn resolve_handler(&self, node_type: StaticNodeType) -> Option<&dyn NodeHandler> {
        self.handlers
            .get(&node_type)
            .map(|handler| handler.as_ref())
    }

    /// Run plugin middleware handlers registered for `phase` in priority order.
    pub async fn run_middleware(
        &self,
        phase: MiddlewarePhase,
        context: &serde_json::Value,
    ) -> ApiResult<()> {
        for middleware in self.plugin_source.middleware(&phase) {
            middleware.handle(&phase, context).await?;
        }
        Ok(())
    }

    /// Whether any plugin node executor is registered for `node_type`.
    pub fn has_plugin_node_executor(&self, node_type: &StaticNodeType) -> bool {
        self.plugin_source
            .node_executor(&node_type_name(node_type))
            .is_some()
    }

    /// Look up a live workflow execution handle by id.
    pub fn workflow_execution(&self, id: &str) -> Option<Arc<WorkflowExecutionEntity>> {
        self.workflow_executions.get(id)
    }

    /// Look up a live agent loop handle by id.
    pub fn agent_loop(&self, id: &str) -> Option<Arc<wf_agent::entity::AgentLoopEntity>> {
        self.agent_loops.get(&wf_types::Id::from(id.to_string()))
    }

    /// Abort every tracked execution driver task (workflow `stream()`
    /// drivers and callback forwarders) still running. Synchronous; the
    /// authoritative teardown path for `Runtime::shutdown` and embedded
    /// consumers. Tasks hold an `Arc` to the context, so `Drop` alone cannot
    /// reach them while they are alive.
    pub fn shutdown(&self) {
        self.execution_tasks.abort_all();
        self.stop_persistence_bridge();
    }

    /// Spawn the event persistence bridge over the current `persistence`
    /// layer, replacing any previous one.
    fn restart_persistence_bridge(&self) {
        self.stop_persistence_bridge();
        let handle =
            crate::infra::event_persistence::EventPersistenceBridge::new(self.persistence.clone())
                .spawn(self.event_bus.clone());
        *wf_common::lock::lock_ok(self.persistence_bridge.lock()) = Some(handle);
    }

    fn stop_persistence_bridge(&self) {
        if let Some(handle) = self.persistence_bridge.lock().expect("bridge lock").take() {
            handle.abort();
        }
    }
}

/// Defensive fallback: abort any tracked driver tasks when the last context
/// reference disappears without an explicit [`ApiContext::shutdown`] (tests,
/// embedded usage). In practice tasks hold `Arc<ApiContext>` so this only
/// fires when no task is alive.
impl Drop for ApiContext {
    fn drop(&mut self) {
        self.shutdown();
    }
}
