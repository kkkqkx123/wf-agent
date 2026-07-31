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
    fn parse_response(&self, body: &str) -> LlmResult<LlmResponseType>;
    fn parse_stream_chunk(&self, data: &str) -> LlmResult<Option<MessageStreamEvent>>;
    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>>;
    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall>;
}

pub fn create_formatter(provider: &LlmProvider) -> Arc<dyn LlmFormatter> {
    match provider {
        LlmProvider::OpenaiChat => Arc::new(OpenaiChatFormatter::new()),
        LlmProvider::OpenaiResponse => Arc::new(OpenaiResponseFormatter::new()),
        LlmProvider::Anthropic => Arc::new(AnthropicFormatter::new()),
        LlmProvider::GeminiNative => Arc::new(GeminiNativeFormatter::new()),
        LlmProvider::GeminiOpenai => Arc::new(GeminiOpenaiFormatter::new()),
    }
}
