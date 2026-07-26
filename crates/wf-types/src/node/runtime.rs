use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeNode {
    pub id: super::super::Id,
    pub node_type: String,
    pub status: String,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub started_at: Option<super::super::Timestamp>,
    pub completed_at: Option<super::super::Timestamp>,
}
