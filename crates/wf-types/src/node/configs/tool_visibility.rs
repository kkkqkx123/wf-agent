use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolVisibilityNodeConfig {
    pub visible_tool_ids: Vec<String>,
    pub mode: String,
}
