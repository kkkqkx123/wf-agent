use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSchema {
    pub id: Option<String>,
    pub description: String,
    pub parameters: super::ToolParameterSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolParameterSchema {
    pub r#type: String,
    pub properties: std::collections::BTreeMap<String, super::ToolPropertySchema>,
    pub required: Vec<String>,
    #[serde(
        rename = "additionalProperties",
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_properties: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    pub id: super::super::Id,
    pub name: String,
    pub description: String,
    pub tool_type: super::ToolType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<ToolParameterSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::ToolMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AvailableTools {
    pub available: Vec<String>,
    /// Tools visible in the initial schema. When absent, all `available`
    /// tools are initially visible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial: Option<Vec<String>>,
    /// Discoverable tools: not in the initial schema; only metadata is
    /// injected into the prompt and calls go through the `general` tool.
    /// Schema injection happens only when the tool is activated via
    /// TOOL_VISIBILITY unblock.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discoverable: Option<Vec<String>>,
    /// Explicitly hidden tools: registered but never exposed to the model
    /// (supplements runtime visibility blocking).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<Vec<String>>,
    /// Escape hatch controlling whether the `general` tool is exposed.
    /// Defaults to auto: exposed iff the discoverable list is non-empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_general_tool: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_workflows: Option<Vec<String>>,
}
