use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use wf_agent::entity::AgentLoopEntity;
use wf_agent::registry::AgentLoopRegistry;
use wf_core::registry::{ConcurrentRegistry, MutableRegistry, Registry};
use wf_core::EventBus;
use wf_execution_shared::execution_state::ExecutionStateManager;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_execution_shared::types::execution_entity::{ExecutionEntity, ExecutionStatus};
use wf_execution_shared::types::execution_instance::ExecutionInstance;
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

use crate::infra::error::ApiError;
use crate::infra::handler_chain::{
    NoopPluginHandlerSource, PluginHandlerSource, PluginNodeAdapter,
};
use crate::infra::persistence::{PersistenceLayer, StorePersistenceLayer};
use crate::infra::tasks::ExecutionTaskRegistry;
use crate::ApiResult;

/// Live execution handle uniting both engines with concrete payloads.
/// Static dispatch on both sides; the variant tag cannot lie because the
/// payload types differ.
pub type LiveExecutionInstance =
    ExecutionInstance<Arc<AgentLoopEntity>, Arc<WorkflowExecutionEntity>>;

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
    /// Shared hook handler registry: hook points and engine signals of
    /// executions launched through this context fire through it.
    pub hook_handler_registry: Option<Arc<HookHandlerRegistry>>,
    /// Optional file checkpoint manager (layertwine-backed): file snapshots
    /// of executions are created/restored through it, and the script handlers
    /// capture workspace changes when it is attached. `None` keeps file
    /// checkpointing disabled.
    file_checkpoint_manager: Option<wf_checkpoint::file::FileCheckpointManager>,
    /// Host-default tool approval configuration (infrastructure config).
    /// When enabled with no caller-supplied handler, executions launched
    /// through this context route every tool call through the persisted
    /// interaction flow. `None` keeps the library default (auto-approve).
    pub tool_approval: Option<wf_types::config::tool_approval::ToolApprovalConfig>,
    /// In-memory stale marks for formal workflows whose upstream references
    /// failed revalidation after an update. Cleared when the workflow is
    /// re-saved formally. Phase three persists this as a lifecycle state.
    pub stale_workflows: Arc<dashmap::DashSet<String>>,
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
            checkpoint_store: Arc::new(storage.checkpoint.store().clone()),
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
            hook_handler_registry: None,
            file_checkpoint_manager: None,
            tool_approval: None,
            stale_workflows: Arc::new(dashmap::DashSet::new()),
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
            checkpoint_store: Arc::new(storage.checkpoint.store().clone()),
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
            hook_handler_registry: None,
            file_checkpoint_manager: None,
            tool_approval: None,
            stale_workflows: Arc::new(dashmap::DashSet::new()),
        };
        // Persist every engine event published on the shared bus.
        ctx.restart_persistence_bridge();
        ctx
    }

    /// Inject the host-default tool approval configuration (from the
    /// infrastructure config layer).
    pub fn with_tool_approval(
        mut self,
        config: wf_types::config::tool_approval::ToolApprovalConfig,
    ) -> Self {
        self.tool_approval = Some(config);
        self
    }

    /// Inject the shared hook handler registry (hook points + engine
    /// signals of executions launched through this context).
    pub fn with_hook_handler_registry(mut self, registry: Arc<HookHandlerRegistry>) -> Self {
        self.hook_handler_registry = Some(registry);
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

    /// Materialize plugin-contributed node handlers as engine handlers for
    /// the node types registered on the plugin source (builtin handlers
    /// always win: a plugin cannot shadow a builtin node type).
    pub fn plugin_handlers(&self) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
        let mut map: HashMap<StaticNodeType, Box<dyn NodeHandler>> = HashMap::new();
        for type_name in self.plugin_source.plugin_node_types() {
            if let Some(node_type) = self.plugin_node_type(&type_name) {
                if self.handlers.contains_key(&node_type) {
                    continue;
                }
                if let Some(executor) = self.plugin_source.node_executor(&type_name) {
                    map.insert(
                        node_type.clone(),
                        Box::new(PluginNodeAdapter::new(executor, node_type)),
                    );
                }
            }
        }
        Arc::new(map)
    }

    /// Parse a plugin node type name into a graph node type. Builtin types
    /// parse strictly; anything else is a plugin-contributed custom type.
    fn plugin_node_type(&self, type_name: &str) -> Option<StaticNodeType> {
        match StaticNodeType::from_str_ci(type_name) {
            Some(node_type) => Some(node_type),
            None => serde_json::from_value::<StaticNodeType>(serde_json::Value::String(
                type_name.to_string(),
            ))
            .ok(),
        }
    }

    /// Resolve the builtin handler for `node_type` from the shared handler
    /// map. The map is immutable after construction, so callers borrow the
    /// resolved handler rather than taking ownership of a per-handler clone.
    ///
    /// Plugin contributions (node executors / middleware) are resolved
    /// through `plugin_source` instead (see [`Self::plugin_handlers`] and
    /// [`Self::run_middleware`]).
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

    /// Look up a live workflow execution handle by id.
    pub fn workflow_execution(&self, id: &str) -> Option<Arc<WorkflowExecutionEntity>> {
        self.workflow_executions.get(id)
    }

    /// Whether a formal workflow is marked stale after an upstream update.
    pub fn is_stale(&self, workflow_id: &str) -> bool {
        self.stale_workflows.contains(workflow_id)
    }

    /// Mark a formal workflow stale. Idempotent.
    pub fn mark_stale(&self, workflow_id: &str) {
        self.stale_workflows.insert(workflow_id.to_string());
    }

    /// Clear the stale mark after a successful formal re-save.
    pub fn clear_stale(&self, workflow_id: &str) {
        self.stale_workflows.remove(workflow_id);
    }

    /// All currently stale workflow ids, sorted for stable output.
    pub fn list_stale(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.stale_workflows.iter().map(|r| r.clone()).collect();
        ids.sort();
        ids
    }

    /// Look up a live agent loop handle by id.
    pub fn agent_loop(&self, id: &str) -> Option<Arc<wf_agent::entity::AgentLoopEntity>> {
        self.agent_loops.get(&wf_types::Id::from(id.to_string()))
    }

    /// Look up any live execution (workflow first, then agent loop) behind
    /// the unified control-plane handle. Engine internals keep using the
    /// typed registries; shared surfaces (status, pause/resume/stop/cancel,
    /// subtree, teardown) go through here instead of branching twice.
    /// Snapshot and progress queries keep typed access because they need
    /// engine-specific state. When one id exists in both registries the
    /// workflow handle wins.
    pub fn execution_instance(&self, id: &str) -> Option<LiveExecutionInstance> {
        if let Some(entity) = self.workflow_executions.get(id) {
            return Some(LiveExecutionInstance::workflow(entity));
        }
        self.agent_loop(id).map(LiveExecutionInstance::agent)
    }

    /// Live status of any execution behind the unified handle.
    pub fn live_execution_status(&self, id: &str) -> ApiResult<ExecutionStatus> {
        self.execution_instance(id)
            .map(|handle| handle.status())
            .ok_or_else(|| ApiError::execution_not_found(id))
    }

    /// Pause any live execution behind the unified handle.
    pub async fn pause_execution(&self, id: &str) -> ApiResult<()> {
        let handle = self
            .execution_instance(id)
            .ok_or_else(|| ApiError::execution_not_found(id))?;
        handle
            .pause()
            .await
            .map_err(|e| ApiError::execution(e.to_string()))
    }

    /// Resume any live execution behind the unified handle.
    pub async fn resume_execution(&self, id: &str) -> ApiResult<()> {
        let handle = self
            .execution_instance(id)
            .ok_or_else(|| ApiError::execution_not_found(id))?;
        handle
            .resume()
            .await
            .map_err(|e| ApiError::execution(e.to_string()))
    }

    /// Stop any live execution behind the unified handle. Idempotent:
    /// stopping an already-terminal execution succeeds.
    pub async fn stop_execution(&self, id: &str) -> ApiResult<()> {
        let handle = self
            .execution_instance(id)
            .ok_or_else(|| ApiError::execution_not_found(id))?;
        handle
            .stop()
            .await
            .map_err(|e| ApiError::execution(e.to_string()))
    }

    /// Cancel any live execution: stop the entity and drop its background
    /// driver task handle when one is tracked (agent task registry and the
    /// shared driver task registry).
    pub async fn cancel_execution(&self, id: &str) -> ApiResult<()> {
        self.stop_execution(id).await?;
        let key = wf_types::Id::from(id.to_string());
        self.agent_loops.abort_task(&key);
        self.execution_tasks.abort(id);
        Ok(())
    }

    /// Every live execution in the hierarchy rooted at `root_id`, including
    /// the root itself, ordered by id for stable output. Membership is
    /// resolved through each execution's root link, falling back to the
    /// ancestor chain so executions with a missing root link are still found.
    pub fn execution_subtree(&self, root_id: &str) -> Vec<LiveExecutionInstance> {
        fn in_subtree(handle: &LiveExecutionInstance, root_id: &str) -> bool {
            if handle.id().as_str() == root_id {
                return true;
            }
            if handle.get_root_execution_id().as_deref() == Some(root_id) {
                return true;
            }
            handle
                .get_ancestors()
                .iter()
                .any(|id| id.as_str() == root_id)
        }
        let mut matches = Vec::new();
        for key in self.workflow_executions.list() {
            if let Some(entity) = self.workflow_executions.get(&key) {
                let handle = LiveExecutionInstance::workflow(entity);
                if in_subtree(&handle, root_id) {
                    matches.push(handle);
                }
            }
        }
        for id in self.agent_loops.get_all_ids() {
            if let Some(entity) = self.agent_loops.get(&id) {
                let handle = LiveExecutionInstance::agent(entity);
                if in_subtree(&handle, root_id) {
                    matches.push(handle);
                }
            }
        }
        matches.sort_by(|a, b| a.id().as_str().cmp(b.id().as_str()));
        matches
    }

    /// Whether a child at `parent_depth` fits the shared nesting gate.
    /// The gate is owned by the agent registry; workflow executions have no
    /// independent quota yet and share this limit for cross-engine nesting.
    pub fn child_depth_allowed(&self, parent_depth: u32) -> bool {
        self.agent_loops.depth_allowed(parent_depth)
    }

    /// Remove terminated executions from both live registries (agent results
    /// and task handles go with them). Termination is judged through the
    /// unified handle so both engines share one terminal definition.
    /// Returns the total removed.
    pub async fn cleanup_terminated_executions(&self) -> usize {
        let mut removed = self.agent_loops.cleanup_terminated().await;
        for key in self.workflow_executions.list() {
            let terminal = self
                .workflow_executions
                .get(&key)
                .map(|entity| LiveExecutionInstance::workflow(entity).is_terminal())
                .unwrap_or(true);
            if terminal {
                self.workflow_executions.unregister(&key);
                removed += 1;
            }
        }
        removed
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
