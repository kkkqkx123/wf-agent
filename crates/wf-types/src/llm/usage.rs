use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsageStats {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    /// Cache-hit input tokens (provider `cached_tokens` / `cache_read_input_tokens`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
    /// Cache-write input tokens (provider `cache_creation_input_tokens`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
}

/// Cost-track record of a single LLM request (real provider usage only).
///
/// The history is the accounting/audit trail: it never mixes in estimated
/// values. When a provider returns no usage the entry is recorded with zero
/// token fields and `estimated: Some(true)` as a marker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsageHistory {
    pub request_id: String,
    pub timestamp: super::super::Timestamp,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    /// True when the entry was filled with an estimation because the
    /// provider reported no usage (cost-track marker, never drives decisions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated: Option<bool>,
}

/// Result of a provider-side token counting API call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenCountResult {
    pub input_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
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
