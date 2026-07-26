#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStaticConfig {
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<HashMap<String, serde_json::Value>>,
}

// ============================================================================
// Agent Loop Execution Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentLoopStatus {
    #[serde(rename = "CREATED")]
    Created,
    #[serde(rename = "RUNNING")]
    Running,
    #[serde(rename = "PAUSED")]
    Paused,
    #[serde(rename = "COMPLETED")]
    Completed,
    #[serde(rename = "FAILED")]
    Failed,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "STOPPED")]
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopResult {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub iterations: u32,
    pub tool_call_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_data: Option<AgentCompletionData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iteration_level_retry_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_loop_retry_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_retry_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_retry_delay_time: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCompletionData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallRecord {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub start_time: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IterationRecord {
    pub iteration: u32,
    pub start_time: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopExecution {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub status: AgentLoopStatus,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub iteration_count: u32,
    #[serde(default)]
    pub tool_call_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<Message>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<AgentLoopResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_execution_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}
