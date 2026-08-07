use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use wf_agent::registry::AgentLoopRegistry;
use wf_core::registry::{ConcurrentRegistry, Registry};
use wf_core::EventBus;
use wf_execution_shared::execution_state::ExecutionStateManager;
use wf_llm::LlmGateway;
use wf_metrics::MetricsRegistry;
use wf_resource::registrar::Registries;
use wf_resource::starter::BundleRegistry;
use wf_storage::backend::StorageBackend;
use wf_storage::context::StorageContext;
use wf_tools::registry::ToolRegistry;
use wf_types::node::StaticNodeType;
use wf_workflow::entity::WorkflowExecutionEntity;
use wf_workflow::handler::NodeHandler;
use wf_workflow::registry::WorkflowExecutionRegistry;

/// Assembled application-facing API context.
///
/// Composes the storage layer (persistent source) with the execution engines
/// (`wf-workflow` / `wf-agent`) and the shared runtime pieces the engines
/// need. Constructed by `wf-runtime` (or an app) through the builder methods;
/// `wf-api` itself never depends on `wf-runtime`.
pub struct ApiContext {
    pub storage: Arc<StorageContext>,
    pub registries: Arc<Registries>,
    pub bundles: Arc<BundleRegistry>,
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
    handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    /// Shared user interaction handler slot (agent config approval / follow-up
    /// wiring). Read and written by `AgentUserInteractionApi`.
    pub user_interaction_handler:
        Arc<tokio::sync::RwLock<Option<Arc<dyn crate::agent_user_interaction::UserInteractionHandler>>>>,
    /// Unified write point for persisted execution records
    /// (`WorkflowExecution` / `AgentExecution`). Wired to the storage
    /// adapters; the engines persist through it and `wf-api` stays read-only.
    pub state_manager: ExecutionStateManager,
}

impl ApiContext {
    pub fn new(
        storage: StorageContext,
        registries: Arc<Registries>,
        bundles: Arc<BundleRegistry>,
    ) -> Self {
        let storage = Arc::new(storage);
        let event_bus = Arc::new(EventBus::new(1024));
        let llm_gateway = Arc::new(LlmGateway::new());
        let handlers = wf_workflow::create_default_handlers(llm_gateway.clone(), None);
        Self {
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
            handlers,
        }
    }

    /// Convenience constructor wiring the pieces a runtime bootstrap already
    /// owns: the shared storage context, resource registries, the shared event
    /// bus, the LLM gateway and the shared tool registry.
    pub fn from_runtime_parts(
        storage: Arc<StorageContext>,
        registries: Arc<Registries>,
        bundles: Arc<BundleRegistry>,
        event_bus: Arc<EventBus>,
        llm_gateway: Arc<LlmGateway>,
        tool_registry: Arc<ToolRegistry>,
        metrics: Option<Arc<MetricsRegistry>>,
    ) -> Self {
        let handlers = wf_workflow::create_default_handlers(llm_gateway.clone(), None);
        Self {
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
            handlers,
        }
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
        handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    ) -> Self {
        self.handlers = handlers;
        self
    }

    pub fn handlers(&self) -> Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>> {
        self.handlers.clone()
    }

    /// Look up a live workflow execution handle by id.
    pub fn workflow_execution(&self, id: &str) -> Option<Arc<WorkflowExecutionEntity>> {
        self.workflow_executions.get(id)
    }

    /// Look up a live agent loop handle by id.
    pub fn agent_loop(&self, id: &str) -> Option<Arc<wf_agent::entity::AgentLoopEntity>> {
        self.agent_loops.get(&wf_types::Id::from(id.to_string()))
    }
}
