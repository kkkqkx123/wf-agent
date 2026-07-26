use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopRuntimeConfig {
    pub max_tool_calls: Option<u32>,
    pub timeout_seconds: Option<u64>,
    pub checkpoint_config: Option<serde_json::Value>,
}
