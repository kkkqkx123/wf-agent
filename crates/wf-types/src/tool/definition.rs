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
    pub properties: std::collections::HashMap<String, super::ToolProperty>,
    pub required: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_workflows: Option<Vec<String>>,
}
