use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCheckpointContentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_state: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_messages: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopCheckpointConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_tool_call: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<AgentCheckpointContentConfig>,
}
