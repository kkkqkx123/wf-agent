use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_messages: Option<Vec<super::super::super::message::Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_messages: Option<Vec<super::super::super::message::Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_message_indices: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_variables: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_variables: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_node_results: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_change: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_change: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_changes: Option<crate::Metadata>,
}
