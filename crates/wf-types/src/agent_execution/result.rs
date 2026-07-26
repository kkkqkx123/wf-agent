use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub iterations: u32,
    pub tool_call_count: u32,
    pub iteration_level_retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_data: Option<serde_json::Value>,
}
