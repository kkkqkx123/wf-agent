use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmResult {
    pub id: Option<String>,
    pub model: String,
    pub content: Option<String>,
    pub message: super::super::message::Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<super::super::message::LlmToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<super::TokenUsageStats>,
    pub finish_reason: Option<String>,
    pub duration: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

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
