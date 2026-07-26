use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallFormat {
    Native,
    Xml,
    JsonWrapped,
    JsonRaw,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallMarkers {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XmlTags {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallFormatConfig {
    pub format: ToolCallFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markers: Option<ToolCallMarkers>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xml_tags: Option<XmlTags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_description: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_examples: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_rules: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_config: Option<HashMap<String, serde_json::Value>>,
}
