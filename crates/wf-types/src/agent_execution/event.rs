use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentStreamEvent {
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: super::super::Timestamp,
}
