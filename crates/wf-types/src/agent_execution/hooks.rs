use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentHookType {
    pub hook_type: String,
    pub config: Option<serde_json::Value>,
}
