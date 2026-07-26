use serde::{Deserialize, Serialize};

use crate::Id;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowHookType {
    BeforeNode,
    AfterNode,
    BeforeToolCall,
    AfterToolCall,
    OnError,
    OnCheckpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookTemplate {
    pub id: Id,
    pub name: String,
    pub description: String,
    pub hook_type: WorkflowHookType,
    pub default_config: Option<serde_json::Value>,
}
