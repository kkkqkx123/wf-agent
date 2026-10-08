use serde::{Deserialize, Serialize};

/// Last-resort wall-clock bound (milliseconds) for a single tool execution
/// when neither the tool definition's `default_timeout_ms` nor an explicit
/// `ToolExecutionOptions::timeout` supplies a value. Shared so the executor
/// wrapper and every caller agree on one fallback rather than repeating the
/// literal.
pub const DEFAULT_TOOL_TIMEOUT_MS: u64 = 30_000;

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
