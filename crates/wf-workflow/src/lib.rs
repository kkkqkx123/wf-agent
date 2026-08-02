pub mod analysis;
pub mod barrier;
pub mod checkpoint;
pub mod coordinator;
pub mod entity;
pub mod error;
pub mod error_analysis;
pub mod execution_callback;
pub mod execution_context;
pub mod executor;
pub mod factory;
pub mod graph;
pub mod handler;
pub mod hook;
pub mod interaction;
pub mod message_context;
pub mod node_validation;
pub mod preprocess;
pub mod protocol_consistency;
pub mod registry;
pub mod state;
pub mod trigger_listener;
pub mod types;
pub mod validation;
pub mod variable;

pub use analysis::{
    analyze_graph, analyze_reachability, detect_cycles, get_nodes_reaching_to, get_reachable_nodes,
    topological_sort, CycleDetectionResult, GraphAnalysis, ReachabilityResult,
    TopologicalSortResult,
};
pub use barrier::{BranchResult, FailureStrategy, ForkOutcome, SyncBarrier};
pub use checkpoint::{CheckpointTiming, NodeCheckpointStrategy, WorkflowCheckpointIntegration};
pub use coordinator::{
    state_transitor::WorkflowStateTransitor, NodeCoordinator, WorkflowCoordinator,
    WorkflowExecutionParams, WorkflowLifecycleCoordinator,
};
pub use entity::WorkflowExecutionEntity;
pub use error::{WorkflowError, WorkflowResult};
pub use error_analysis::{analyze_workflow_error, workflow_error_record};
pub use execution_callback::WorkflowExecutionCallback;
pub use execution_context::{ExecutionContextRegistry, WriteBackError};
pub use executor::WorkflowExecutor;
pub use factory::WorkflowExecutionBuilder;
pub use handler::{
    agent_loop::AgentLoopHandler,
    context_processor::ContextProcessorHandler,
    fork_join::ForkHandler,
    fork_join::JoinHandler,
    interactive_script::InteractiveScriptHandler,
    llm::LlmHandler,
    loop_handler::LoopEndHandler,
    loop_handler::LoopStartHandler,
    route::RouteHandler,
    script::ScriptHandler,
    start_end::EndHandler,
    start_end::StartHandler,
    subgraph::SubgraphHandler,
    sync::SyncHandler,
    tool_visibility::ToolVisibilityHandler,
    trigger::{ContinueFromTriggerHandler, StartFromTriggerHandler, TriggerCoordinator},
    variable::VariableHandler,
    HandlerRegistry, NodeHandler, NodeHandlerResult,
};
pub use hook::WorkflowHookHandler;
pub use interaction::{
    complete_interaction, interaction_registry, register_interaction, InteractionRegistry,
};
pub use message_context::{
    append_context, get_context, has_context, register_context, DEFAULT_CONTEXT_ID,
};
pub use registry::{
    create_execution_registry, create_graph_registry, lookup_graph, lookup_script, register_graph,
    register_script, ScriptDefinition, WorkflowExecutionPool, WorkflowExecutionRegistry,
    WorkflowGraphRegistry,
};
pub use state::{NodeExecutionRecord, WorkflowExecutionState, WorkflowExecutionStateSnapshot};
pub use trigger_listener::{SubworkflowRunner, TriggerEventListener, TriggerTemplateRegistry};
pub use types::WorkflowExecutionParams as WorkflowExecutionParamsType;
pub use validation::{GraphValidator, ValidationError, ValidationResult};
pub use variable::{create_variable_store, VariableResolver, VariableStore};

use std::collections::HashMap;
use std::sync::Arc;

use wf_llm::LlmGateway;

pub use wf_types::node::StaticNodeType;

pub fn create_default_handlers(
    gateway: Arc<LlmGateway>,
) -> Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>> {
    let mut registry = HandlerRegistry::new();
    registry.register_defaults(gateway);
    registry.into_arc()
}
