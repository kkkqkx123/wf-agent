use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionErrorRecord {
    pub node_id: Option<String>,
    pub error_message: String,
    pub error_type: String,
    pub timestamp: super::super::Timestamp,
}
