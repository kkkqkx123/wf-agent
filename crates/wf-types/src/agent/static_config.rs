use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentHookStatic {
    pub hook_type: String,
    pub condition: Option<String>,
    pub event_name: String,
    pub event_payload: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub weight: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTriggerStatic {
    pub trigger_type: String,
    pub config: Option<serde_json::Value>,
}
