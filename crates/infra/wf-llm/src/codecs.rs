use crate::error::LlmResult;
use std::sync::Arc;
use wf_types::llm::{
    LlmFormat, LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent,
};
use wf_types::tool::Tool;

pub mod anthropic;
pub mod gemini_native;
pub mod openai_chat;
pub mod openai_response;
pub mod shared;

pub use anthropic::AnthropicCodec;
pub use gemini_native::GeminiNativeCodec;
pub use openai_chat::OpenaiChatCodec;
pub use openai_response::OpenaiResponseCodec;

pub trait LlmCodec: Send + Sync {
    fn build_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<reqwest::Request>;
    /// Parse a non-streaming response. `request` carries the effective tool
    /// call protocol so the codec can route to text-mode parsing when needed.
    fn parse_response(&self, body: &str, request: &LlmRequest) -> LlmResult<LlmResponseType>;
    fn parse_stream_chunk(&self, data: &str) -> LlmResult<Option<MessageStreamEvent>>;
    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>>;
    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall>;

    /// Build a count-tokens request. Returns `Ok(None)` when the format
    /// does not support a token counting API (the caller falls back to
    /// an estimate). Default implementation returns `None`.
    ///
    /// Format support: Anthropic (`POST /messages/count_tokens`), OpenAI
    /// Responses (`POST /responses/input_tokens`) and Gemini native
    /// (`POST /models/*:countTokens`) expose a counting API. OpenAI Chat
    /// Completions and custom OpenAI-compatible formats have no counting
    /// endpoint and keep the default `None`.
    fn build_count_tokens_request(
        &self,
        _request: &LlmRequest,
        _profile: &LlmProfile,
    ) -> LlmResult<Option<reqwest::Request>> {
        Ok(None)
    }

    /// Parse a count-tokens response body into input tokens. Default reads
    /// `input_tokens` (Anthropic and OpenAI Responses shape); formats
    /// with a different shape (Gemini `totalTokens`) override it.
    fn parse_count_tokens_response(&self, body: &serde_json::Value) -> LlmResult<u32> {
        Ok(body
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32)
    }
}

/// Create the codec for a built-in format. Custom formats are not
/// handled here: they are resolved through `CodecRegistry` and yield
/// `UnsupportedFormat` when passed to this factory.
pub fn create_codec(format: &LlmFormat) -> LlmResult<Arc<dyn LlmCodec>> {
    match format {
        LlmFormat::OpenaiChat => Ok(Arc::new(OpenaiChatCodec::new())),
        LlmFormat::OpenaiResponse => Ok(Arc::new(OpenaiResponseCodec::new())),
        LlmFormat::Anthropic => Ok(Arc::new(AnthropicCodec::new())),
        LlmFormat::GeminiNative => Ok(Arc::new(GeminiNativeCodec::new())),
        LlmFormat::Custom(_) => Err(crate::error::LlmError::UnsupportedFormat(format.clone())),
    }
}
