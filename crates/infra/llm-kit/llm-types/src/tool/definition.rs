//! Wire tool declaration: the minimal name/description/parameters shape
//! exchanged with LLM providers.
//!
//! This is the protocol-level view consumed by codecs, prompt rendering and
//! token estimation. Agent-side governance (identity, kind, metadata,
//! enablement, timeouts) lives on the rich tool definition owned by the
//! host workspace and is mapped to this shape at the request boundary.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolParameterSchema {
    pub r#type: String,
    pub properties: std::collections::BTreeMap<String, super::ToolPropertySchema>,
    pub required: Vec<String>,
    #[serde(
        rename = "additionalProperties",
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_properties: Option<bool>,
}

/// Minimal wire declaration of one tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<ToolParameterSchema>,
}
