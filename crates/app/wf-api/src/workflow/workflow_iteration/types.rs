use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct ToolDependencyView {
    pub tool_name: String,
    pub call_count: u32,
}

/// One step of the reconstructed execution path.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPathStepView {
    pub step_id: String,
    /// `node_execution` | `condition_check` | `branch_decision` | `tool_call`.
    pub r#type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    pub timestamp: i64,
}

/// Execution path of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowExecutionPathView {
    pub path_id: String,
    pub description: String,
    pub steps: Vec<ExecutionPathStepView>,
    pub is_optimal: bool,
}

/// Extended node execution record.
#[derive(Debug, Clone, Serialize)]
pub struct ExtendedNodeExecutionRecordView {
    pub execution_id: String,
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    /// `pending` | `running` | `completed` | `failed` | `skipped` | `cancelled`.
    pub status: String,
    pub start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_dependencies: Vec<ToolDependencyView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// Filter over extended node execution records.
#[derive(Debug, Clone, Default)]
pub struct ExtendedNodeExecutionFilter {
    pub execution_ids: Option<Vec<String>>,
    pub node_id: Option<String>,
    pub node_type: Option<String>,
    pub status: Option<String>,
    pub has_errors: bool,
    pub min_duration: Option<i64>,
    pub time_range: Option<(Option<i64>, Option<i64>)>,
}

/// Aggregated node execution statistics.
#[derive(Debug, Clone, Serialize)]
pub struct NodeExecutionStats {
    pub total_executions: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub average_execution_time: i64,
    pub min_execution_time: i64,
    pub max_execution_time: i64,
    pub retry_total: u32,
}

/// Optimization opportunity for a node execution.
#[derive(Debug, Clone, Serialize)]
pub struct OptimizationOpportunity {
    pub node_id: String,
    pub node_name: String,
    pub description: String,
    pub impact_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_improvement: Option<String>,
}

/// All extended node execution records of a workflow execution, one per
/// distinct node (the latest attempt wins), in start-time order.
#[derive(Debug, Clone, Serialize)]
pub struct LlmReasoningRecordView {
    pub step_id: String,
    /// `thinking` | `planning` | `analyzing` | `evaluating` | `synthesizing`.
    pub reasoning_type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_entities: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conclusions: Vec<String>,
}

/// LLM reasoning path of a node: one record per LLM attempt of the node,
/// extracted from the recorded input/output chain.
pub struct RawNodeAttempt {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub success: bool,
    pub error: Option<String>,
    pub input: Option<Value>,
    pub output: Option<Value>,
    pub metadata: Option<Value>,
}
