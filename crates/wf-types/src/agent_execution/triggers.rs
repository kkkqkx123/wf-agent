use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTrigger {
    pub trigger_type: String,
    pub condition: Option<String>,
    pub action: Option<serde_json::Value>,
}
