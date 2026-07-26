use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallProtocolConfig {
    pub format: super::ToolCallFormat,
    pub parallel_tool_calls: Option<bool>,
}
