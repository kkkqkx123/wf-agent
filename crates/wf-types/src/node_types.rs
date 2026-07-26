// ============================================================================
// Node Type Enum
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeType {
    Start,
    End,
    Variable,
    Fork,
    Join,
    Sync,
    Subgraph,
    EmbedGraph,
    Script,
    InteractiveScript,
    Llm,
    ToolVisibility,
    UserInteraction,
    Route,
    ContextProcessor,
    LoopStart,
    LoopEnd,
    AgentLoop,
    StartFromTrigger,
    ContinueFromTrigger,
}

// ============================================================================
// Shared Node Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeIdentity {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StaticNodeDisplayProps {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeExecutionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<ExecutionHook>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_before_execute: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_after_execute: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_delay_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exponential_backoff: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeNodeContext {
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_workflow_id: Option<String>,
    #[serde(default)]
    pub outgoing_edge_ids: Vec<String>,
    #[serde(default)]
    pub incoming_edge_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_node: Option<serde_json::Value>,
}

// ============================================================================
// Static Node
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StaticNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: NodeType,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub config: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<ExecutionHook>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_before_execute: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_after_execute: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_delay_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exponential_backoff: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<VariableDefinition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
}

// ============================================================================
// Runtime Node
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: NodeType,
    pub config: serde_json::Value,
    #[serde(default)]
    pub outgoing_edge_ids: Vec<String>,
    #[serde(default)]
    pub incoming_edge_ids: Vec<String>,
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_node: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<ExecutionHook>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_before_execute: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_after_execute: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_delay_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exponential_backoff: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<HashMap<String, serde_json::Value>>,
}

// ============================================================================
// Node Configurations
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RouteNodeConfig {
    pub routes: Vec<RouteRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_target_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RouteRule {
    pub condition: Condition,
    pub target_node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VariableNodeConfig {
    pub variable_name: String,
    #[serde(rename = "type")]
    pub var_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForkNodeConfig {
    pub fork_paths: Vec<ForkPath>,
    pub fork_strategy: ForkStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_strategy: Option<ForkFailureStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_failed_branches: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_execution_timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_branch_timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForkPath {
    pub path_id: String,
    pub child_node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ForkStrategy {
    #[serde(rename = "serial")]
    Serial,
    #[serde(rename = "parallel")]
    Parallel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ForkFailureStrategy {
    #[serde(rename = "fail-fast")]
    FailFast,
    #[serde(rename = "continue-on-error")]
    ContinueOnError,
    #[serde(rename = "fail-on-threshold")]
    FailOnThreshold,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JoinNodeConfig {
    pub fork_path_ids: Vec<String>,
    pub join_strategy: JoinStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    pub main_path_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable_outputs: Option<Vec<WorkflowVariableOutput>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_outputs: Option<Vec<JoinMessageOutput>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_outputs: Option<Vec<WorkflowDataOutput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JoinStrategy {
    #[serde(rename = "ALL_COMPLETED")]
    AllCompleted,
    #[serde(rename = "ANY_COMPLETED")]
    AnyCompleted,
    #[serde(rename = "ALL_FAILED")]
    AllFailed,
    #[serde(rename = "ANY_FAILED")]
    AnyFailed,
    #[serde(rename = "SUCCESS_COUNT_THRESHOLD")]
    SuccessCountThreshold,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JoinMessageOutput {
    #[serde(flatten)]
    pub output: WorkflowMessageOutput,
    pub source_path_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoopStartNodeConfig {
    pub loop_id: String,
    #[serde(default)]
    pub variable_inputs: Vec<LoopVariableInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_source: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_iteration_failure: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoopVariableInput {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_variable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoopEndNodeConfig {
    pub loop_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_condition: Option<Condition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_start_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScriptNodeConfig {
    pub script_name: String,
    pub risk: ScriptRiskLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#inline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_config: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_mapping: Option<ScriptOutputMappingList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ScriptOutputMappingList {
    Single(ScriptOutputMapping),
    Multiple(Vec<ScriptOutputMapping>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScriptOutputMapping {
    pub target: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InteractiveScriptNodeConfig {
    pub script_name: String,
    pub risk: ScriptRiskLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_mode: Option<InteractionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_patterns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rounds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round_timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_config: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_mapping: Option<ScriptOutputMappingList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InteractionMode {
    #[serde(rename = "blocking")]
    Blocking,
    #[serde(rename = "llm-assisted")]
    LlmAssisted,
    #[serde(rename = "hybrid")]
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmNodeConfig {
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls_per_request: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_format: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolVisibilityNodeConfig {
    pub action: ToolVisibilityAction,
    pub tool_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolVisibilityAction {
    #[serde(rename = "block")]
    Block,
    #[serde(rename = "unblock")]
    Unblock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserInteractionNodeConfig {
    pub operation_type: UserInteractionOperationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<UserInteractionVariable>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<UserInteractionMessage>,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UserInteractionOperationType {
    #[serde(rename = "UPDATE_VARIABLES")]
    UpdateVariables,
    #[serde(rename = "ADD_MESSAGE")]
    AddMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserInteractionVariable {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserInteractionMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextProcessorNodeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_config: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable_operation: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_options: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubgraphNodeConfig {
    pub subgraph_id: String,
    #[serde(default)]
    pub r#async: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    #[serde(default)]
    pub variable_inputs: Vec<WorkflowVariableInput>,
    #[serde(default)]
    pub data_inputs: Vec<WorkflowDataInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_passing: Option<MessagePassingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EmbedGraphNodeConfig {
    pub embed_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncNodeConfig {
    pub source_path_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_path_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable_mappings: Option<Vec<SyncVariableMapping>>,
    #[serde(default)]
    pub data_inputs: Vec<WorkflowDataInput>,
    #[serde(default)]
    pub message_inputs: Vec<WorkflowMessageInput>,
    #[serde(default)]
    pub wait_for_completion: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pair_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable_exchanges: Option<Vec<SyncVariableExchange>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncVariableMapping {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncVariableExchange {
    pub variable_name: String,
    pub source_path_id: String,
    pub target_path_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopNodeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_config: Option<AgentLoopInlineConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopInlineConfig {
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<Vec<String>>,
    #[serde(default)]
    pub data_inputs: Vec<WorkflowDataInput>,
    #[serde(default)]
    pub message_inputs: Vec<WorkflowMessageInput>,
    #[serde(default)]
    pub message_outputs: Vec<WorkflowMessageOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_timeout: Option<u64>,
}

// ============================================================================
// Node Output Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartNodeOutput {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndNodeOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VariableNodeOutput {
    pub variable_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<serde_json::Value>,
    pub new_value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForkNodeOutput {
    pub launched_branches: Vec<ForkBranchInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForkBranchInfo {
    pub path_id: String,
    pub child_node_id: String,
    pub strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JoinNodeOutput {
    pub completed_branches: Vec<String>,
    pub failed_branches: Vec<JoinFailedBranch>,
    pub skipped_branches: Vec<String>,
    pub strategy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregated_output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JoinFailedBranch {
    pub path_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncNodeOutput {
    pub synced_from_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_variables: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_variable_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_data_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_message_count: Option<u32>,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubgraphNodeOutput {
    pub execution_result: SubgraphExecutionResult,
    pub duration: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubgraphExecutionResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScriptNodeOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InteractiveScriptNodeOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmNodeOutput {
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LlmNodeToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmNodeToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolVisibilityNodeOutput {
    pub action: String,
    pub tool_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserInteractionNodeOutput {
    pub operation_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_variables: Option<Vec<UpdatedVariableInfo>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_messages: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdatedVariableInfo {
    pub variable_name: String,
    pub new_value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RouteNodeOutput {
    pub next_node_id: String,
    pub evaluated_conditions: Vec<EvaluatedCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedCondition {
    pub condition: String,
    pub result: bool,
    pub target_node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextProcessorNodeOutput {
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_variables: Option<Vec<ModifiedVariableInfo>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_time: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModifiedVariableInfo {
    pub name: String,
    pub new_value: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub var_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoopStartNodeOutput {
    pub loop_id: String,
    pub iteration_count: u32,
    pub max_iterations: u32,
    pub has_more_iterations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoopEndNodeOutput {
    pub loop_id: String,
    pub break_triggered: bool,
    pub iteration_count: u32,
    pub next_iteration: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EmbedGraphNodeOutput {
    pub embed_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopNodeOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_response: Option<String>,
    pub tool_call_count: u32,
    pub iteration_count: u32,
}
