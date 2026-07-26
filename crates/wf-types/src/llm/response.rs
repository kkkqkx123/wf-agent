use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatResponse {
    pub id: Option<String>,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<super::TokenUsageStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatChoice {
    pub index: u32,
    pub message: serde_json::Value,
    pub finish_reason: Option<String>,
}
