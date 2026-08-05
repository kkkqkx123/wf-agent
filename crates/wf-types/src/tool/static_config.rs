use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolProperty {
    pub name: String,
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<super::ToolRiskLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_approvable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_checkpoint: Option<super::CheckpointTiming>,
}
