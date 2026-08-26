use serde::{Deserialize, Serialize};

use crate::message::Message;

/// Workflow checkpoint delta.
/// Uses the camelCase wire format; status/current-node transitions carry
/// the `{ from, to }` pair.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCheckpointDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_messages: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_messages: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_message_indices: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_variables: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_variables: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_node_results: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_change: Option<super::super::FieldChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_change: Option<super::super::FieldChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_changes: Option<crate::Metadata>,
}
