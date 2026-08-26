use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityAction {
    Block,
    Unblock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolVisibilityNodeConfig {
    pub action: VisibilityAction,
    pub tool_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolVisibilityNodeOutput {
    pub action: VisibilityAction,
    pub tool_ids: Vec<String>,
}
