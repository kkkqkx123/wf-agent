use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::Metadata;

/// Agent loop delta. Field values use
/// the camelCase wire format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCheckpointDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_messages: Option<Vec<Message>>,
    /// Sequence coordinate of the first message in `added_messages` when the
    /// delta carries a suffix rather than a full replacement. Absent means a
    /// legacy full-replacement delta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_message_base_seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_iterations: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_change: Option<super::super::FieldChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_changes: Option<Metadata>,
}
