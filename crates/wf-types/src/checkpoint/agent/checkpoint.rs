use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_messages: Option<Vec<super::super::super::message::Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_iterations: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_change: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_changes: Option<crate::Metadata>,
}
