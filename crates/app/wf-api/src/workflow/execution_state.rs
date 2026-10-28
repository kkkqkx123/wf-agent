//! Execution-state queries over workflow executions and agent loops,
//! split by concern. This file keeps the module root so external paths
//! (`crate::workflow::execution_state::...`) stay unchanged.
//!
//! - `views`: serializable view structs (DTOs)
//! - `accessor`: live-entity adapter for the shared state recorder
//! - `workflow`: workflow execution state / variables / transitions queries
//! - `context`: execution context, call stack, snapshots and analysis

mod accessor;
mod context;
#[cfg(test)]
mod tests;
mod views;
mod workflow;

use wf_types::ExecutionStatus;

pub use accessor::WorkflowStateAccessor;
pub use context::{
    workflow_execution_analyze_state_transitions, workflow_execution_get_call_stack,
    workflow_execution_get_context_evolution, workflow_execution_get_context_transitions,
    workflow_execution_get_execution_context, workflow_execution_get_key_context_snapshots,
    workflow_execution_get_memory_usage, workflow_execution_get_node_input_context,
    workflow_execution_get_node_transitions, workflow_execution_get_variable_snapshots,
    workflow_execution_get_variable_snapshots_by_time_range,
};
pub use views::{
    CommonTransitionView, ContextEvolutionView, ContextStateTransitionView,
    ExecutionContextSnapshotView, NodeExecutionRecordView, NodeInputContextView,
    StateTransitionView, VariableSnapshotView, VariableValueSnapshotView, WorkflowCallStackView,
    WorkflowExecutionStateView, WorkflowStackFrameView, WorkflowStateTransitionAnalysisView,
};
pub use workflow::{
    workflow_execution_get_state, workflow_execution_status_transitions,
    workflow_execution_variables,
};

/// Parse a persisted string status onto the typed contract. Delegates to the
/// canonical status parser so that every known status (including `timeout`)
/// resolves identically across crates; unknown values return `None` so
/// corrupt records surface instead of being coerced.
pub fn parse_status(status: &str) -> Option<ExecutionStatus> {
    ExecutionStatus::from_wire(status).ok()
}

/// The serialized status string (serde snake_case form).
pub fn status_str(status: &ExecutionStatus) -> &'static str {
    status.as_str()
}
