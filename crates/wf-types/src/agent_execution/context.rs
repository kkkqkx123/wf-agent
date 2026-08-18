use serde::{Deserialize, Serialize};

use crate::Id;
use crate::Metadata;
use crate::Timestamp;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamicPromptContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u32>,
    pub current_iteration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamicPromptInjection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub static_system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_user_context: Option<String>,
}
