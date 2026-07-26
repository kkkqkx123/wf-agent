// ============================================================================
// Edge
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    #[serde(rename = "type")]
    pub edge_type: EdgeType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdgeType {
    #[serde(rename = "DEFAULT")]
    Default,
    #[serde(rename = "CONDITIONAL")]
    Conditional,
}

// ============================================================================
// Workflow Template
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTemplate {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub template_type: WorkflowTemplateType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub nodes: Vec<StaticNode>,
    pub edges: Vec<Edge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<VariableDefinition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<TriggerOrReference>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggered_subworkflow_config: Option<TriggeredSubworkflowConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<WorkflowConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<WorkflowMetadata>,
    pub version: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<AvailableTools>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkflowTemplateType {
    #[serde(rename = "TRIGGERED_SUBWORKFLOW")]
    TriggeredSubworkflow,
    #[serde(rename = "STANDALONE")]
    Standalone,
    #[serde(rename = "DEPENDENT")]
    Dependent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<HashMap<String, serde_json::Value>>,
}

// ============================================================================
// Workflow Config
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_checkpoints: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_config: Option<CheckpointConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<WorkflowRetryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_approval: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<AvailableTools>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_messages: Option<Vec<Message>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_template_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_template_variables: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_contexts: Option<Vec<StaticContextConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_before_node: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_after_node: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRetryPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_delay: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_multiplier: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StaticContextConfig {
    pub id: String,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TriggeredSubworkflowConfig {
    pub trigger_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_filter: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_checkpoints: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_config: Option<CheckpointConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
}

// ============================================================================
// Boundary Configs
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStartConfig {
    #[serde(default)]
    pub variable_inputs: Vec<WorkflowVariableInput>,
    #[serde(default)]
    pub message_inputs: Vec<WorkflowMessageInput>,
    #[serde(default)]
    pub data_inputs: Vec<WorkflowDataInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowEndConfig {
    #[serde(default)]
    pub variable_outputs: Vec<WorkflowVariableOutput>,
    #[serde(default)]
    pub message_outputs: Vec<WorkflowMessageOutput>,
    #[serde(default)]
    pub data_outputs: Vec<WorkflowDataOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowVariableInput {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_variable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowVariableOutput {
    pub name: String,
    pub source_variable: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMessageInput {
    pub context_id: String,
    #[serde(default)]
    pub include_history: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMessageOutput {
    pub context_id: String,
    pub target_context: String,
    #[serde(default)]
    pub append: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDataInput {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDataOutput {
    pub name: String,
    pub output_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ============================================================================
// Available Tools
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvailableTools {
    pub available: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_workflows: Option<Vec<String>>,
}
