use crate::error::LlmResult;
use std::sync::Arc;
use wf_types::llm::{
    LlmProfile, LlmProvider, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent,
};
use wf_types::tool::Tool;

pub mod anthropic;
pub mod gemini_native;
pub mod gemini_openai;
pub mod openai_chat;
pub mod openai_response;
pub mod shared;

pub use anthropic::AnthropicFormatter;
pub use gemini_native::GeminiNativeFormatter;
pub use gemini_openai::GeminiOpenaiFormatter;
pub use openai_chat::OpenaiChatFormatter;
pub use openai_response::OpenaiResponseFormatter;

pub trait LlmFormatter: Send + Sync {
    fn build_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<reqwest::Request>;
    /// Parse a non-streaming response. `request` carries the effective tool
    /// call format so the formatter can route to text-mode parsing when needed.
    fn parse_response(&self, body: &str, request: &LlmRequest) -> LlmResult<LlmResponseType>;
    fn parse_stream_chunk(&self, data: &str) -> LlmResult<Option<MessageStreamEvent>>;
    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>>;
    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall>;

    /// Build a count-tokens request. Returns `Ok(None)` when the provider
    /// does not support a token counting API (the caller falls back to
    /// an estimate). Default implementation returns `None`.
    fn build_count_tokens_request(
        &self,
        _request: &LlmRequest,
        _profile: &LlmProfile,
    ) -> LlmResult<Option<reqwest::Request>> {
        Ok(None)
    }
}

/// Create the formatter for a built-in provider. Custom providers are not
/// handled here: they are resolved through `FormatterRegistry` and yield
/// `UnsupportedProvider` when passed to this factory.
pub fn create_formatter(provider: &LlmProvider) -> LlmResult<Arc<dyn LlmFormatter>> {
    match provider {
        LlmProvider::OpenaiChat => Ok(Arc::new(OpenaiChatFormatter::new())),
        LlmProvider::OpenaiResponse => Ok(Arc::new(OpenaiResponseFormatter::new())),
        LlmProvider::Anthropic => Ok(Arc::new(AnthropicFormatter::new())),
        LlmProvider::GeminiNative => Ok(Arc::new(GeminiNativeFormatter::new())),
        LlmProvider::GeminiOpenai => Ok(Arc::new(GeminiOpenaiFormatter::new())),
        LlmProvider::Custom(_) => Err(crate::error::LlmError::UnsupportedProvider(
            provider.clone(),
        )),
    }
}
