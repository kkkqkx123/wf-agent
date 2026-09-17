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

impl ToolRiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolRiskLevel::ReadOnly => "read_only",
            ToolRiskLevel::Write => "write",
            ToolRiskLevel::Execute => "execute",
            ToolRiskLevel::Mcp => "mcp",
            ToolRiskLevel::Network => "network",
            ToolRiskLevel::System => "system",
            ToolRiskLevel::Interaction => "interaction",
        }
    }

    pub fn parse_case_insensitive(level: &str) -> Option<Self> {
        match level.to_ascii_lowercase().as_str() {
            "read_only" | "readonly" => Some(ToolRiskLevel::ReadOnly),
            "write" => Some(ToolRiskLevel::Write),
            "execute" => Some(ToolRiskLevel::Execute),
            "mcp" => Some(ToolRiskLevel::Mcp),
            "network" => Some(ToolRiskLevel::Network),
            "system" => Some(ToolRiskLevel::System),
            "interaction" => Some(ToolRiskLevel::Interaction),
            _ => None,
        }
    }
}
