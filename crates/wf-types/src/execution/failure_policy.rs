use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FailureAction {
    Continue,
    Retry,
    Fallback,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetryPolicy {
    pub enabled: bool,
    pub max_retries: u32,
    pub base_delay_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_delay_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backoff_multiplier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jitter: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FallbackPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_value: Option<serde_json::Value>,
    pub log_fallback: bool,
    pub continue_after_fallback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FailureContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FailurePolicyConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_policy: Option<FallbackPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_retryable_errors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics_enabled: Option<bool>,
}
