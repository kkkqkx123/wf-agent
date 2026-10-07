//! Execution-state queries over workflow executions and agent loops,
//! split by concern. This file keeps the module root so external paths
//! (`crate::workflow::execution_state::...`) stay unchanged.
//!
//! - `views`: serializable view structs (DTOs)
//! - `accessor`: live-entity adapter for the shared state recorder
//! - `workflow`: workflow execution state / variables / transitions queries
//! - `context`: execution context, call stack, snapshots and analysis
//! - `agent`: agent loop state queries

mod accessor;
mod agent;
mod context;
#[cfg(test)]
mod tests;
mod views;
mod workflow;

pub use accessor::WorkflowStateAccessor;
pub use agent::{
    agent_execution_get_state, agent_execution_iteration_history, agent_execution_variables,
    parse_status, status_str,
};
pub use context::{
    workflow_execution_analyze_state_transitions, workflow_execution_get_call_stack,
    workflow_execution_get_context_evolution, workflow_execution_get_context_transitions,
    workflow_execution_get_execution_context, workflow_execution_get_key_context_snapshots,
    workflow_execution_get_memory_usage, workflow_execution_get_node_input_context,
    workflow_execution_get_node_transitions, workflow_execution_get_variable_snapshots,
    workflow_execution_get_variable_snapshots_by_time_range,
};
pub use views::{
    AgentLoopStateView, CommonTransitionView, ContextEvolutionView, ContextStateTransitionView,
    ExecutionContextSnapshotView, IterationRecordView, NodeExecutionRecordView,
    NodeInputContextView, StateTransitionView, ToolCallRecordView, VariableSnapshotView,
    VariableValueSnapshotView, WorkflowCallStackView, WorkflowExecutionStateView,
    WorkflowStackFrameView, WorkflowStateTransitionAnalysisView,
};
pub use workflow::{
    workflow_execution_get_state, workflow_execution_status_transitions,
    workflow_execution_variables,
};
