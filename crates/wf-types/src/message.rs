use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub id: crate::Id,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: crate::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
}

pub mod batch_management_operation;
pub mod batch_snapshot;
pub mod message_array;
pub mod message_context;
pub mod message_mark_map;
pub mod message_operations;

pub use batch_management_operation::*;
pub use batch_snapshot::*;
pub use message_array::*;
pub use message_context::*;
pub use message_mark_map::*;
pub use message_operations::*;
