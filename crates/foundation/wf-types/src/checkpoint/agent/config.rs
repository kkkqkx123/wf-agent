use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCheckpointContentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_state: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_messages: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCheckpointConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_tool_call: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_compression: Option<bool>,
    /// Checkpoint every N appended conversation messages. `None` or 0
    /// disables message-level checkpoints (tool boundaries already cover
    /// most intra-iteration moments); enabled only by explicit opt-in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_interval: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<AgentCheckpointContentConfig>,
}
