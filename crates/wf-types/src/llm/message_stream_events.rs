use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamEvent {
    pub event_type: String,
    pub data: serde_json::Value,
}
