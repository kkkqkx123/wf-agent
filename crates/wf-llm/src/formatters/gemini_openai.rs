use super::shared;
use super::LlmFormatter;
use crate::error::LlmResult;
use reqwest::Method;
use wf_types::llm::{LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent};
use wf_types::tool::Tool;

pub struct GeminiOpenaiFormatter {
    base_url: String,
}

impl Default for GeminiOpenaiFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl GeminiOpenaiFormatter {
    pub fn new() -> Self {
        Self {
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
        }
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self { base_url }
    }
}

impl LlmFormatter for GeminiOpenaiFormatter {
    fn build_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<reqwest::Request> {
        let url = format!(
            "{}/chat/completions",
            profile.base_url.as_deref().unwrap_or(&self.base_url)
        );

        let use_text_mode = shared::is_text_mode(request);

        let messages = if use_text_mode {
            let (_, filtered) = crate::tool_format::extract_system_message(&request.messages);
            let content = shared::text_mode_system_content(request);
            let mut converted = shared::convert_openai_messages(&filtered);
            if !content.is_empty() {
                converted.insert(0, serde_json::json!({"role": "system", "content": content}));
            }
            converted
        } else {
            shared::convert_openai_messages(&request.messages)
        };

        let mut body = serde_json::json!({
            "model": profile.model,
            "messages": messages,
        });

        shared::merge_and_apply_params(&mut body, profile, &request.parameters);

        if !use_text_mode {
            if let Some(tools) = &request.tools {
                body["tools"] = serde_json::json!(shared::convert_openai_tools(tools)?);
            }
        }

        if request.stream == Some(true) {
            body["stream"] = serde_json::json!(true);
        }

        shared::apply_custom_body(&mut body, profile);

        let mut req_builder = reqwest::Client::new()
            .request(Method::POST, &url)
            .header("Content-Type", "application/json")
            .json(&body);

        req_builder = shared::apply_auth_and_headers(req_builder, profile, "bearer");

        req_builder
            .build()
            .map_err(crate::error::LlmError::HttpError)
    }

    fn parse_response(&self, body: &str, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        let mut result = shared::parse_openai_chat_response(body)?;
        if shared::is_text_mode(request) {
            if let Some(content) = result.content.clone() {
                let calls = shared::parse_text_tool_calls(request, &content);
                if !calls.is_empty() {
                    result.tool_calls = Some(calls.clone());
                    result.message.tool_calls = Some(calls);
                }
            }
        }
        Ok(result)
    }

    fn parse_stream_chunk(&self, data: &str) -> LlmResult<Option<MessageStreamEvent>> {
        shared::parse_openai_stream_chunk(data)
    }

    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>> {
        shared::convert_openai_tools(tools)
    }

    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall> {
        result.tool_calls.clone().unwrap_or_default()
    }
}
