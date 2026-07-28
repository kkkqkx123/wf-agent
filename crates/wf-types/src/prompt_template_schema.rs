use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptVariableDefinitionSchema {
    pub name: String,
    pub description: Option<String>,
    pub default_value: Option<String>,
    pub required: Option<bool>,
    pub variable_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptTemplateSchema {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub variables: Option<Vec<PromptVariableDefinitionSchema>>,
    pub version: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub fn is_valid_prompt_template(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.contains_key("id") && map.contains_key("name") && map.contains_key("content")
        }
        _ => false,
    }
}
