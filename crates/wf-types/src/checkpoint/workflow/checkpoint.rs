use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointDelta {
    pub node_id: String,
    pub delta_type: String,
    pub data: serde_json::Value,
    pub timestamp: super::super::super::Timestamp,
}
