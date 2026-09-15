use serde::{Deserialize, Serialize};

/// A model advertized by a provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
}
