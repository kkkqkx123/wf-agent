use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamStats {
    pub chunk_count: u32,
    pub time_to_first_chunk: i64,
    pub stream_duration: i64,
    pub total_duration: i64,
}

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
    pub metadata: Option<super::super::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_stats: Option<StreamStats>,
}
