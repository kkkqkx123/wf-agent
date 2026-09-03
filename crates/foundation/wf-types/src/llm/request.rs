use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeadLoopDetectionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Character-length checkpoints at which detection runs
    /// (default: [500, 1000, 2000]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoints: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_sequence_window: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_repeat_unit_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_repeat_count: Option<u32>,
    /// Minimum number of consecutive equal elements (paragraphs / normalized
    /// lines) that must repeat to be flagged as a loop (default: 6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_period_elements: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_period_length: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallProtocolViolationPolicy {
    Ignore,
    Warn,
    Fail,
    AutoConvert,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmRequest {
    pub profile_id: String,
    pub messages: Vec<super::super::message::Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<super::generation::LlmGenerationParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<super::super::tool::Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_format: Option<super::ToolCallFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_tool_call_format: Option<super::tool_call_format::ToolCallFormatConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation_policy: Option<ToolCallProtocolViolationPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dead_loop_detection: Option<DeadLoopDetectionConfig>,
    /// Set by the gateway when a locked tool call format conflict was resolved
    /// via the `auto_convert` policy; observed by formatters for observability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_auto_converted: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
}
