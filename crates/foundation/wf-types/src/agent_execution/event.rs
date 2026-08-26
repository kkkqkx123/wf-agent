use serde::{Deserialize, Serialize};

use crate::Timestamp;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentStreamEvent {
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: Timestamp,
}
