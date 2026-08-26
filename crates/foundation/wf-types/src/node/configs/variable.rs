use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VariableNodeType {
    Number,
    String,
    Boolean,
    Array,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableNodeConfig {
    pub variable_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_type: Option<VariableNodeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableNodeOutput {
    pub variable_name: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: serde_json::Value,
}
