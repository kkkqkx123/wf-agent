use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopDelta {
    pub delta_type: String,
    pub data: serde_json::Value,
    pub iteration: u32,
}
