use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsageStats {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsageHistory {
    pub request_id: String,
    pub timestamp: super::super::Timestamp,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsageStatistics {
    pub total_requests: u64,
    pub average_tokens: f64,
    pub max_tokens: u32,
    pub min_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
}
