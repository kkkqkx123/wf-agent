use serde::{Deserialize, Serialize};

/// Streaming options for LLM requests (e.g. whether to include usage in stream).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmStreamOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_usage: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmProfile {
    pub id: String,
    pub name: String,
    pub provider: super::LlmProvider,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_delay: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_format: Option<super::tool_call_format::ToolCallFormatConfig>,
    /// Authentication type: "native" (provider-specific headers) or "bearer"
    /// (Authorization: Bearer). Defaults to "native".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,
    /// Custom headers to add to every request (simple key-value map).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_headers: Option<crate::Metadata>,
    /// Custom body fields to deep-merge into the request body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_body: Option<serde_json::Value>,
    /// Whether custom body merging is enabled (default: true when custom_body present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_body_enabled: Option<bool>,
    /// Query parameters to append to the request URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_params: Option<crate::Metadata>,
    /// Streaming options (e.g. include_usage for OpenAI).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<LlmStreamOptions>,
    /// Model's maximum input context window size in tokens.
    ///
    /// This is a **model capability** property, distinct from the per-request
    /// `max_tokens` (output limit). It tells the runtime how many input tokens
    /// the model can accept before truncation or compression is needed.
    ///
    /// Common values: 128000 (GPT-4o, Gemini 2.5 Flash), 200000 (Claude 4.5),
    /// 256000 (Gemini 2.5 Pro), 1048576 (Gemini 2.5 Pro extended).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window_size: Option<u32>,
}
