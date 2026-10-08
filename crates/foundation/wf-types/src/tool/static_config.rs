use serde::{Deserialize, Serialize};

pub use llm_types::tool::static_config::ToolMetadata;

/// Legacy weak property declaration. Deprecated: use the strongly-typed
/// [`super::ToolPropertySchema`] instead. Kept only for deserializing
/// historical configurations.
#[deprecated(note = "use ToolPropertySchema instead of the weak ToolProperty")]
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
