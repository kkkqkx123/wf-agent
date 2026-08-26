use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserInteractionStorageMetadata {
    pub id: super::super::Id,
    pub execution_id: super::super::Id,
    pub interaction_type: String,
    pub status: String,
    pub request_data: Value,
    pub response_data: Option<Value>,
    pub result_data: Option<Value>,
    pub error: Option<String>,
    pub created_at: super::super::Timestamp,
    pub responded_at: Option<super::super::Timestamp>,
}
