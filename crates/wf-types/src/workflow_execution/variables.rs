use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableDefinition {
    pub name: String,
    pub value: serde_json::Value,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableScope {
    pub variables: HashMap<String, serde_json::Value>,
    pub parent_scope: Option<Box<VariableScope>>,
}
