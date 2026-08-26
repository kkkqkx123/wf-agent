use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolRiskLevel {
    ReadOnly,
    Write,
    Execute,
    Mcp,
    Network,
    System,
    Interaction,
}
