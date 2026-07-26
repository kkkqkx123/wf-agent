use serde::{Deserialize, Serialize};

use super::super::agent_execution::AgentHookType;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentHookStatic {
    pub hook_type: AgentHookType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_checkpoint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_description: Option<String>,
}
