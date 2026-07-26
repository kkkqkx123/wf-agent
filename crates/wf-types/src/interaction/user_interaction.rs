use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserInteractionRequest {
    pub id: super::super::Id,
    pub prompt: String,
    pub context: Option<serde_json::Value>,
    pub response: Option<String>,
    pub status: String,
}
