use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use wf_types::ExecutionStatus;

/// One node execution attempt of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct NodeExecutionRecordView {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub success: bool,
    pub error: Option<String>,
}

/// Snapshot of a workflow execution's state.
///
/// Data sources: the live entity in the in-memory registry (full state, up
/// to completion) or the persisted `WorkflowExecution` record (fields the
/// persistence boundary kept). `source` tells the consumer which boundary the
/// view was built from.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowExecutionStateView {
    pub execution_id: String,
    pub workflow_id: Option<String>,
    pub status: ExecutionStatus,
    pub current_node_id: Option<String>,
    pub completed_nodes: Vec<String>,
    pub node_execution_history: Vec<NodeExecutionRecordView>,
    pub variables: BTreeMap<String, Value>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub error: Option<String>,
    pub source: String,
}

/// A reconstructed state transition (derived from lifecycle events).
#[derive(Debug, Clone, Serialize)]
pub struct StateTransitionView {
    pub from: String,
    pub to: String,
    pub timestamp: i64,
}

/// One tool call recorded by an agent iteration.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallRecordView {
    pub name: String,
    pub duration_ms: i64,
    pub success: bool,
}

/// One agent loop iteration record.
#[derive(Debug, Clone, Serialize)]
pub struct IterationRecordView {
    pub iteration: u32,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub tool_call_count: u32,
    pub tool_calls: Vec<ToolCallRecordView>,
}

/// Snapshot of an agent loop's execution state.
#[derive(Debug, Clone, Serialize)]
pub struct AgentLoopStateView {
    pub agent_loop_id: String,
    pub status: ExecutionStatus,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    pub iteration_history: Vec<IterationRecordView>,
    pub variables: BTreeMap<String, Value>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub error: Option<String>,
    pub source: String,
}

/// One variable value at a specific point in time.
#[derive(Debug, Clone, Serialize)]
pub struct VariableValueSnapshotView {
    pub name: String,
    pub value: Value,
    pub r#type: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_no: Option<u32>,
}

/// Variable snapshot at an execution point in time.
///
/// Reconstructed from the node execution history: one snapshot per node
/// execution (with the variables observed at that point) plus an initial
/// snapshot when the execution started.
#[derive(Debug, Clone, Serialize)]
pub struct VariableSnapshotView {
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub variables: Vec<VariableValueSnapshotView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_id: Option<String>,
}

/// One call-stack frame of an in-progress execution.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowStackFrameView {
    pub frame_id: String,
    /// `node_execution` | `tool_call` | `condition_check` | `branch` |
    /// `subworkflow` | `loop`.
    pub r#type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    pub frame_variables: BTreeMap<String, Value>,
    pub entry_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_frame_id: Option<String>,
}

/// Call-stack snapshot of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowCallStackView {
    pub execution_id: String,
    pub timestamp: i64,
    pub frames: Vec<WorkflowStackFrameView>,
    pub depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_id: Option<String>,
}

/// Execution context snapshot.
///
/// A point-in-time view of the execution: the active node, variable context,
/// progress and the reconstructed call stack. `pending_nodes` is only
/// populated when the execution graph can be resolved.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionContextSnapshotView {
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_name: Option<String>,
    pub global_variables: BTreeMap<String, Value>,
    pub completed_nodes: Vec<String>,
    pub pending_nodes: Vec<String>,
    pub skipped_nodes: Vec<String>,
    /// Execution progress (0.0 - 100.0).
    pub execution_progress: f64,
    pub call_stack: Vec<WorkflowStackFrameView>,
    /// Estimated resident memory usage of the execution state (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage: Option<i64>,
}

/// Context state transition between nodes.
#[derive(Debug, Clone, Serialize)]
pub struct ContextStateTransitionView {
    pub transition_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_node: Option<String>,
    /// `sequential` | `conditional_branch` | `loop` | `parallel_fork` |
    /// `join` | `completion`.
    pub transition_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub timestamp: i64,
}

/// Context evolution of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct ContextEvolutionView {
    pub execution_id: String,
    pub start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    pub transitions: Vec<ContextStateTransitionView>,
    /// Number of variable changes. Per-mutation history is not retained by
    /// the state boundary, so this is a lower bound (the number of distinct
    /// variables present at the end of execution).
    pub total_variable_changes: u64,
}

/// One frequent state transition pair.
#[derive(Debug, Clone, Serialize)]
pub struct CommonTransitionView {
    pub from: String,
    pub to: String,
    pub count: u64,
    pub frequency: f64,
}

/// State-transition analysis of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowStateTransitionAnalysisView {
    pub total_transitions: u64,
    pub common_transitions: Vec<CommonTransitionView>,
    pub state_entry_count: BTreeMap<String, u64>,
    pub average_time_in_state: BTreeMap<String, i64>,
}

/// Input context of a node at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct NodeInputContextView {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub input_parameters: BTreeMap<String, Value>,
    pub timestamp: i64,
    pub available_variables: Vec<VariableValueSnapshotView>,
}
