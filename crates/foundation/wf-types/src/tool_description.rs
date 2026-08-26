use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolParameterDescription {
    pub name: String,
    pub r#type: String,
    pub required: bool,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDescriptionData {
    pub id: String,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub description: String,
    pub parameters: Vec<ToolParameterDescription>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tips: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<String>>,
}
