pub mod barrier;
pub mod checkpoint;
pub mod coordinator;
pub mod entity;
pub mod error;
pub mod executor;
pub mod factory;
pub mod graph;
pub mod handler;
pub mod hook;
pub mod registry;
pub mod state;
pub mod types;
pub mod validation;
pub mod variable;

pub use barrier::{BranchResult, FailureStrategy, ForkOutcome, SyncBarrier};
pub use checkpoint::{CheckpointTiming, NodeCheckpointStrategy, WorkflowCheckpointIntegration};
pub use coordinator::{
    state_transitor::WorkflowStateTransitor, NodeCoordinator, WorkflowCoordinator,
    WorkflowExecutionParams, WorkflowLifecycleCoordinator,
};
pub use entity::WorkflowExecutionEntity;
pub use error::{WorkflowError, WorkflowResult};
pub use executor::WorkflowExecutor;
pub use factory::WorkflowExecutionBuilder;
pub use handler::{
    agent_loop::AgentLoopHandler, context_processor::ContextProcessorHandler,
    fork_join::ForkHandler, fork_join::JoinHandler, interactive_script::InteractiveScriptHandler,
    llm::LlmHandler, loop_handler::LoopEndHandler, loop_handler::LoopStartHandler,
    route::RouteHandler, script::ScriptHandler, start_end::ContinueFromTriggerHandler,
    start_end::EndHandler, start_end::StartHandler, subgraph::SubgraphHandler, sync::SyncHandler,
    tool_visibility::ToolVisibilityHandler, variable::VariableHandler, HandlerRegistry,
    NodeHandler, NodeHandlerResult,
};
pub use hook::WorkflowHookHandler;
pub use registry::{
    create_execution_registry, create_graph_registry, WorkflowExecutionPool,
    WorkflowExecutionRegistry, WorkflowGraphRegistry,
};
pub use state::{WorkflowExecutionState, WorkflowExecutionStateSnapshot};
pub use types::WorkflowExecutionParams as WorkflowExecutionParamsType;
pub use variable::{create_variable_store, VariableResolver, VariableStore};

use std::collections::HashMap;
use std::sync::Arc;

pub use wf_types::node::StaticNodeType;

pub fn create_default_handlers() -> Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>> {
    let mut registry = HandlerRegistry::new();
    registry.register_defaults();
    registry.into_arc()
}
