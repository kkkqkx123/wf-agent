use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::Metadata;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCheckpointDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_messages: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_iterations: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_change: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_changes: Option<Metadata>,
}
