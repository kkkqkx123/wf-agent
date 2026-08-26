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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_iterations: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_change: Option<super::super::FieldChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_changes: Option<Metadata>,
}
