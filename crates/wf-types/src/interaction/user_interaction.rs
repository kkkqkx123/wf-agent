use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UserInteractionOperationType {
    ToolApproval,
    AskFollowupQuestion,
    ScriptInteraction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserInteractionRequest {
    pub interaction_id: super::super::Id,
    pub operation_type: UserInteractionOperationType,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserInteractionResponse {
    pub interaction_id: super::super::Id,
    pub input_data: serde_json::Value,
    pub timestamp: super::super::Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserInteractionResult {
    pub interaction_id: super::super::Id,
    pub operation_type: UserInteractionOperationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<serde_json::Value>>,
    pub timestamp: super::super::Timestamp,
}
