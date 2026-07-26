use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    pub id: super::super::Id,
    pub name: String,
    pub description: String,
    pub tool_type: super::ToolType,
    pub config: Option<serde_json::Value>,
}
