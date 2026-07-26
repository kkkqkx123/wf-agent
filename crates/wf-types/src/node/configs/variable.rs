use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableNodeConfig {
    pub operation: String,
    pub variable_name: String,
    pub value: Option<serde_json::Value>,
    pub expression: Option<String>,
}
