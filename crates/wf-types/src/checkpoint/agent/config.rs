use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopCheckpointConfig {
    pub enabled: bool,
    pub interval_iterations: Option<u32>,
    pub on_error: Option<bool>,
    pub on_tool_call: Option<bool>,
}
