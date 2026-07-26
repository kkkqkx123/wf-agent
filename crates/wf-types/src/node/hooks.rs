use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeHook {
    pub hook_type: String,
    pub condition: Option<serde_json::Value>,
    pub event_name: String,
    pub enabled: Option<bool>,
}
