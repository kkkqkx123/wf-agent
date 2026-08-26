use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextProcessorNodeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_operation: Option<super::variable_operation::VariableOperationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageOperationOutput {
    pub operation: String,
    pub message_count: u32,
    pub source_context: Option<String>,
    pub target_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextProcessorNodeOutput {
    #[serde(flatten)]
    pub inner: serde_json::Value,
}
