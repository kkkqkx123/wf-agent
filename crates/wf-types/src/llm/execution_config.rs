use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LlmExecutionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tool_calls_per_request: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_token_tracking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_warning_threshold: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_limit: Option<u32>,
}
