// ============================================================================
// Execution Status
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    #[serde(rename = "CREATED")]
    Created,
    #[serde(rename = "RUNNING")]
    Running,
    #[serde(rename = "PAUSED")]
    Paused,
    #[serde(rename = "STOPPED")]
    Stopped,
    #[serde(rename = "COMPLETED")]
    Completed,
    #[serde(rename = "FAILED")]
    Failed,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "TIMEOUT")]
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "RUNNING")]
    Running,
    #[serde(rename = "COMPLETED")]
    Completed,
    #[serde(rename = "FAILED")]
    Failed,
    #[serde(rename = "SKIPPED")]
    Skipped,
    #[serde(rename = "PAUSED")]
    Paused,
}

// ============================================================================
// Workflow Execution Type
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkflowExecutionType {
    #[serde(rename = "MAIN")]
    Main,
    #[serde(rename = "FORK_JOIN")]
    ForkJoin,
    #[serde(rename = "TRIGGERED_SUBWORKFLOW")]
    TriggeredSubworkflow,
    #[serde(rename = "SUBGRAPH")]
    Subgraph,
}

// ============================================================================
// Execution Contexts
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForkJoinContext {
    pub fork_id: String,
    pub fork_path_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TriggeredSubworkflowContext {
    pub parent_execution_id: String,
    pub child_execution_ids: Vec<String>,
    pub triggered_subworkflow_id: String,
}

// ============================================================================
// Workflow Execution
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecution {
    pub id: String,
    pub workflow_id: String,
    pub workflow_version: String,
    pub status: ExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_execution_id: Option<String>,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_results: Option<Vec<NodeExecutionResult>>,
    #[serde(default)]
    pub errors: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_data: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_type: Option<WorkflowExecutionType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_join_context: Option<ForkJoinContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggered_subworkflow_context: Option<TriggeredSubworkflowContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<ExecutionHierarchy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeExecutionResult {
    pub node_id: String,
    pub node_type: String,
    pub status: String,
    pub step: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_delays: Option<Vec<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_retry_time: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_used: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_recovered: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timed_out: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeExecution {
    pub id: String,
    pub node_id: String,
    pub workflow_execution_id: String,
    pub status: NodeStatus,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(default)]
    pub retry_count: u32,
}

// ============================================================================
// Execution Mode
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkflowExecutionMode {
    #[serde(rename = "SYNC")]
    Sync,
    #[serde(rename = "ASYNC")]
    Async,
    #[serde(rename = "blocking")]
    Blocking,
    #[serde(rename = "foreground")]
    Foreground,
    #[serde(rename = "background")]
    Background,
}

// ============================================================================
// Workflow Execution Options
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_execution_time: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_checkpoints: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pause_duration: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_node_retry: Option<DefaultNodeRetry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_budget: Option<RetryBudgetOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_delay_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exponential_backoff: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<FallbackOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DefaultNodeRetry {
    pub max_retries: u32,
    pub retry_delay: u64,
    pub exponential_backoff: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RetryBudgetOption {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_budget_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FallbackOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

// ============================================================================
// Workflow Execution Result
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionResult {
    pub execution_id: String,
    pub output: HashMap<String, serde_json::Value>,
    pub execution_time: i64,
    pub node_results: Vec<NodeExecutionResult>,
    pub metadata: WorkflowExecutionResultMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ExecutionErrorEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_retry_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_retry_delay_time: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_retry_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_retry_delay_time: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionResultMetadata {
    pub status: ExecutionStatus,
    pub start_time: i64,
    pub end_time: i64,
    pub execution_time: i64,
    pub node_count: u32,
    pub error_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interruption_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupted_at_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionErrorEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FailurePolicy {
    pub on_failure: FailureAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_delay_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exponential_backoff: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FailureAction {
    #[serde(rename = "fail")]
    Fail,
    #[serde(rename = "retry")]
    Retry,
    #[serde(rename = "continue")]
    Continue,
    #[serde(rename = "fallback")]
    Fallback,
}

// ============================================================================
// Execution Hierarchy
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionHierarchy {
    pub root_execution_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentExecutionContext>,
    #[serde(default)]
    pub children: Vec<ChildExecutionReference>,
    pub depth: u32,
    pub root_execution_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "parentType", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ParentExecutionContext {
    #[serde(rename = "WORKFLOW")]
    Workflow(WorkflowParentContext),
    #[serde(rename = "AGENT_LOOP")]
    AgentLoop(AgentLoopParentContext),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowParentContext {
    pub parent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopParentContext {
    pub parent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_purpose: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "childType", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChildExecutionReference {
    #[serde(rename = "WORKFLOW")]
    Workflow(WorkflowChildContext),
    #[serde(rename = "AGENT_LOOP")]
    AgentLoop(AgentLoopChildContext),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowChildContext {
    pub child_id: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_path_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_interruption: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopChildContext {
    pub child_id: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_interruption: Option<bool>,
}
