use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::message::Message;

/// Append-only diff for one named message context: only the messages newly
/// present in the current snapshot (deduplicated by message id). Applying it
/// extends the base context history rather than replacing it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageContextDelta {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub added_messages: Vec<Message>,
}

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
    /// Added message contexts, keyed by context id. Each context carries only
    /// the messages newly present in `current` (message-id-deduplicated), so
    /// applying the delta appends to the base context history instead of
    /// replacing it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_contexts: Option<HashMap<String, MessageContextDelta>>,
    /// Node results replaced wholesale (first appearance, or when the whole
    /// map is swapped). For per-key changes see `modified_node_results`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_node_results: Option<serde_json::Value>,
    /// Node results changed at the key level only (unchanged nodes kept).
    /// Keyed by node id; values replace the matching entry in the base map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_node_results: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_change: Option<super::super::FieldChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_change: Option<super::super::FieldChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_changes: Option<crate::Metadata>,
}
