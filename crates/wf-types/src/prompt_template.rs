use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptVariableDefinition {
    pub name: String,
    pub r#type: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<PromptVariableDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragments: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateFillRule {
    pub template_id: String,
    pub variable_mapping: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragment_mapping: Option<HashMap<String, String>>,
}
