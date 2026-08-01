use super::shared;
use super::LlmFormatter;
use crate::error::LlmResult;
use reqwest::Method;
use wf_types::llm::{LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent};
use wf_types::tool::Tool;

pub struct OpenaiChatFormatter {
    base_url: String,
}

impl Default for OpenaiChatFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenaiChatFormatter {
    pub fn new() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self { base_url }
    }

    fn inject_tool_declarations(&self, messages: &mut Vec<serde_json::Value>, tools: &[Tool]) {
        let mut tool_text = String::from("\n\n## Available Tools\n");
        for tool in tools {
            tool_text.push_str(&format!("\n### {}\n{}\n", tool.name, tool.description));
            if let Some(ref params) = tool.parameters {
                if let Ok(params_str) = serde_json::to_string_pretty(params) {
                    tool_text.push_str(&format!("Parameters:\n{}\n", params_str));
                }
            }
        }
        tool_text.push_str("\n\nWhen using a tool, respond with XML:\n<tool_use>\n  <tool_name>tool_name</tool_name>\n  <parameters>\n    <param_name>value</param_name>\n  </parameters>\n</tool_use>");

        let system_idx = messages
            .iter()
            .position(|m| m.get("role").and_then(|v| v.as_str()) == Some("system"));

        if let Some(idx) = system_idx {
            if let Some(existing) = messages[idx].get("content").and_then(|v| v.as_str()) {
                let new_content = format!("{}{}", existing, tool_text);
                messages[idx]["content"] = serde_json::json!(new_content);
            } else {
                messages[idx]["content"] = serde_json::json!(tool_text.trim().to_string());
            }
        } else {
            messages.insert(
                0,
                serde_json::json!({
                    "role": "system",
                    "content": tool_text.trim().to_string(),
                }),
            );
        }
    }
}

impl LlmFormatter for OpenaiChatFormatter {
    fn build_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<reqwest::Request> {
        let url = format!(
            "{}/chat/completions",
            profile.base_url.as_deref().unwrap_or(&self.base_url)
        );

        let mut messages = shared::convert_openai_messages(&request.messages);

        let use_text_mode = matches!(request.tool_call_format, Some(ref f) if *f != wf_types::llm::ToolCallFormat::Native);

        if use_text_mode {
            if let Some(ref tools) = request.tools {
                self.inject_tool_declarations(&mut messages, tools);
            }
        }

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
            body["stream_options"] = serde_json::json!({"include_usage": true});
        }

        let mut req_builder = reqwest::Client::new()
            .request(Method::POST, &url)
            .header("Content-Type", "application/json")
            .json(&body);

        req_builder = shared::add_auth_and_headers(req_builder, profile, "bearer");

        req_builder
            .build()
            .map_err(crate::error::LlmError::HttpError)
    }

    fn parse_response(&self, body: &str, _request: &LlmRequest) -> LlmResult<LlmResponseType> {
        shared::parse_openai_chat_response(body)
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
