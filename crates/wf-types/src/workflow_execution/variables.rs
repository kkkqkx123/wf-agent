use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VariableValueType {
    Number,
    String,
    Boolean,
    Array,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableDefinition {
    pub name: String,
    pub value: serde_json::Value,
    pub r#type: Option<VariableValueType>,
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableScope {
    pub variables: HashMap<String, serde_json::Value>,
    pub parent_scope: Option<Box<VariableScope>>,
}
