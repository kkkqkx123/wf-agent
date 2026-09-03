pub mod analysis;
pub mod barrier;
pub mod checkpoint;
pub mod config_parse;
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
pub mod loop_state;
pub mod message_context;
pub mod node_validation;
pub mod persistence;
pub mod preprocess;
pub mod protocol_consistency;
pub mod reference_closure;
pub mod registry;
pub mod state;
pub mod trigger_internal;
pub mod trigger_listener;
pub mod trigger_states;
pub mod types;
pub mod validation;
pub mod variable;

pub use analysis::{
    analyze_graph, analyze_reachability, detect_cycles, get_nodes_reaching_to, get_reachable_nodes,
    topological_sort, CycleDetectionResult, GraphAnalysis, ReachabilityResult,
    TopologicalSortResult,
};
pub use barrier::{BranchResult, FailureStrategy, ForkOutcome};
pub use checkpoint::{
    NodeCheckpointStrategy, WorkflowCheckpointIntegration, WorkflowCheckpointTiming,
};
pub use coordinator::{
    state_transitor::WorkflowStateTransitor, NodeCoordinator, WorkflowCoordinator,
    WorkflowExecutionParams, WorkflowLifecycleCoordinator,
};
pub use entity::WorkflowExecutionEntity;
pub use error::{WorkflowError, WorkflowResult};
pub use error_analysis::{
    analyze_workflow_error, analyze_workflow_error_pattern, chained_workflow_error_record,
    workflow_error_record, WorkflowErrorPattern,
};
pub use execution_callback::WorkflowExecutionCallback;
pub use execution_context::{ExecutionContextRegistry, WriteBackError};
pub use executor::WorkflowExecutor;
pub use factory::WorkflowExecutionBuilder;
pub use handler::{
    agent_loop::AgentLoopHandler,
    context_processor::ContextProcessorHandler,
    embed_boundary::{EmbedEndHandler, EmbedStartHandler},
    fork_join::ForkHandler,
    fork_join::JoinHandler,
    interactive_script::InteractiveScriptHandler,
    llm::LlmHandler,
    loop_handler::LoopEndHandler,
    loop_handler::LoopStartHandler,
    message_node::{ContinueFromMessageHandler, StartFromMessageHandler},
    route::RouteHandler,
    script::ScriptHandler,
    start_end::EndHandler,
    start_end::StartHandler,
    subgraph::SubgraphHandler,
    sync::SyncHandler,
    tool_visibility::ToolVisibilityHandler,
    trigger::{TriggerContext, TriggerCoordinator},
    variable::VariableHandler,
    HandlerRegistry, NodeHandler, NodeHandlerResult,
};
pub use hook::WorkflowHookHandler;
pub use interaction::{
    complete_interaction, interaction_registry, register_interaction, InteractionRegistry,
    InteractionWait,
};
pub use loop_state::{
    current_item, current_loop, enter_loop, exit_loop, find_loop, iterable_len, loop_condition_met,
    mark_iteration_failed, update_loop, LoopState, MAX_ITERATIONS_CAP,
};
pub use message_context::{
    append_context, get_context, has_context, register_context, DEFAULT_CONTEXT_ID,
};
pub use persistence::build_workflow_execution;
pub use reference_closure::{ReferenceClosureReport, ReferenceContext, MAX_REFERENCE_DEPTH};
pub use registry::{
    create_execution_registry, create_graph_registry, lookup_graph, lookup_script, register_graph,
    register_script, ScriptDefinition, ScriptRegistry, WorkflowExecutionRegistry,
    WorkflowGraphRegistry,
};
pub use state::{
    NodeExecutionRecord, WorkflowExecutionState, WorkflowExecutionStateSnapshot,
    WorkflowInterruptionStatistics,
};
pub use trigger_listener::{
    SubworkflowRunner, TriggerActionRunner, TriggerEventListener, TriggerTemplateRegistry,
};
pub use trigger_states::{TriggerStateRecord, TriggerStateRegistry};
pub use types::WorkflowExecutionParams as WorkflowExecutionParamsType;
pub use validation::{format_validation_report, GraphValidator, ValidationError, ValidationResult};
pub use variable::{
    convert_variable_type, create_variable_store, evaluate_expression, ExprEvaluator,
    ExpressionError, VariableResolver, VariableStore,
};

use std::collections::HashMap;
use std::sync::Arc;

use wf_llm::LlmGateway;

pub use wf_types::node::StaticNodeType;

/// Build the standard handler set. `sandbox` (shared, precompiled sandbox
/// runtime) is injected into the script handlers; `None` lets them fall back
/// to their own default runtime.
pub fn create_default_handlers(
    gateway: Arc<LlmGateway>,
    sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    create_default_handlers_with_file_checkpoint(gateway, sandbox, None)
}

/// Like [`create_default_handlers`], additionally attaching the
/// file-checkpoint manager to the script handlers for script-change capture.
pub fn create_default_handlers_with_file_checkpoint(
    gateway: Arc<LlmGateway>,
    sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
    file_checkpoint: Option<wf_checkpoint::file::FileCheckpointManager>,
) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    let mut registry = HandlerRegistry::new();
    registry.register_defaults_with_file_checkpoint(gateway, sandbox, file_checkpoint);
    registry.into_arc()
}
