//! Query and view types of the agent loop registry.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use wf_types::ExecutionStatus;

/// Agent loop query filter.
#[derive(Debug, Clone, Default)]
pub struct AgentLoopFilter {
    pub ids: Option<Vec<String>>,
    pub status: Option<ExecutionStatus>,
    pub profile_id: Option<String>,
    pub tags: Option<Vec<String>>,
    /// Inclusive `(start, end)` creation timeframe.
    pub created_at_range: Option<(Option<i64>, Option<i64>)>,
}

/// Digest of an agent loop execution.
#[derive(Debug, Clone, Serialize)]
pub struct AgentLoopSummary {
    pub id: String,
    pub status: ExecutionStatus,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    /// Execution this loop was spawned under, absent for a root loop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_execution_id: Option<String>,
}

/// Iteration detail for agent loop history.
#[derive(Debug, Clone, Serialize)]
pub struct IterationDetail {
    pub iteration: u32,
    pub start_time: i64,
    pub end_time: i64,
    /// Duration in ms (-1 while still in progress).
    pub duration: i64,
    pub tool_call_count: u32,
    pub tool_calls: Vec<wf_agent::state::ToolCallRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_content: Option<String>,
}

/// Agent loop iteration history summary.
#[derive(Debug, Clone, Serialize)]
pub struct IterationHistorySummary {
    pub total_iterations: u32,
    pub total_tool_calls: u32,
    pub total_duration: i64,
    pub average_duration: i64,
    pub status: ExecutionStatus,
}

/// Timeline entry type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTimelineEntryType {
    ExecutionStart,
    ExecutionEnd,
    ExecutionCompleted,
    ExecutionFailed,
    ExecutionCancelled,
    ExecutionStopped,
    ExecutionTimeout,
    IterationStart,
    IterationEnd,
    Error,
    InterruptionPause,
    InterruptionResume,
    InterruptionStop,
    InterruptionTimeout,
}

/// Execution timeline entry.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionTimelineEntry {
    pub id: String,
    pub timestamp: i64,
    pub r#type: ExecutionTimelineEntryType,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_severity: Option<String>,
}

/// One point of a variable's history.
#[derive(Debug, Clone, Serialize)]
pub struct VariableHistoryEntry {
    pub timestamp: i64,
    pub name: String,
    pub value: Value,
    pub iteration: u32,
    pub change: VariableChange,
}

/// How a variable changed between consecutive snapshots.
#[derive(Debug, Clone, Serialize)]
pub struct VariableChange {
    pub from: Option<Value>,
    pub to: Value,
}

/// Context evolution entry.
#[derive(Debug, Clone, Serialize)]
pub struct ContextEvolutionEntry {
    pub timestamp: i64,
    pub iteration: u32,
    pub status: ExecutionStatus,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<u32>,
}

/// Aggregated execution statistics.
#[derive(Debug, Clone, Serialize)]
pub struct AgentExecutionStatistics {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub success_rate: f64,
    pub avg_duration: i64,
    pub total_iterations: u32,
    pub avg_iterations_per_execution: f64,
    pub total_tool_calls: u32,
    pub avg_tool_calls_per_execution: f64,
}

/// Tool call in an execution path.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallInPath {
    pub name: String,
    pub status: String,
    pub start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
}

/// Iteration in an execution path.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPathIteration {
    pub iteration: u32,
    pub tool_calls: Vec<ToolCallInPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
}

/// Execution path.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPath {
    pub execution_id: String,
    pub status: ExecutionStatus,
    pub total_iterations: u32,
    pub iterations: Vec<ExecutionPathIteration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<i64>,
}

/// Statistics of the agent loop registry.
#[derive(Debug, Clone, Serialize)]
pub struct AgentLoopStatistics {
    pub total: usize,
    pub by_status: BTreeMap<String, usize>,
}
