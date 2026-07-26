use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerCondition {
    pub condition_type: String,
    pub expression: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerAction {
    pub action_type: String,
    pub config: Option<serde_json::Value>,
}
