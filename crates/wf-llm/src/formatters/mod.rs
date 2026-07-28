use async_trait::async_trait;
use reqwest::Method;
use std::sync::Arc;
use wf_types::llm::{LlmRequest, LlmResult as LlmResponseType, LlmProfile, LlmProvider, MessageStreamEvent};
use crate::error::LlmResult;
use wf_types::tool::Tool;

pub mod anthropic;
pub mod gemini_native;
pub mod gemini_openai;

pub use anthropic::AnthropicFormatter;
pub use gemini_native::GeminiNativeFormatter;
pub use gemini_openai::GeminiOpenaiFormatter;

#[async_trait]
pub trait LlmFormatter: Send + Sync {
    fn build_request(&self, request: &LlmRequest, profile: &LlmProfile) -> LlmResult<reqwest::Request>;
    fn parse_response(&self, body: &str) -> LlmResult<LlmResponseType>;
    fn parse_stream_chunk(&self, data: &str) -> LlmResult<Option<MessageStreamEvent>>;
    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>>;
    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall>;
}

pub fn create_formatter(provider: &LlmProvider) -> Arc<dyn LlmFormatter> {
    match provider {
        LlmProvider::OpenaiChat => Arc::new(OpenaiChatFormatter::new()),
        LlmProvider::OpenaiResponse => Arc::new(OpenaiChatFormatter::new()),
        LlmProvider::Anthropic => Arc::new(AnthropicFormatter::new()),
        LlmProvider::GeminiNative => Arc::new(GeminiNativeFormatter::new()),
        LlmProvider::GeminiOpenai => Arc::new(GeminiOpenaiFormatter::new()),
    }
}

pub struct OpenaiChatFormatter {
    base_url: String,
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
}

#[async_trait]
impl LlmFormatter for OpenaiChatFormatter {
    fn build_request(&self, request: &LlmRequest, profile: &LlmProfile) -> LlmResult<reqwest::Request> {
        let url = format!("{}/chat/completions", profile.base_url.as_deref().unwrap_or(&self.base_url));

        let mut body = serde_json::json!({
            "model": profile.model,
            "messages": request.messages,
        });

        if let Some(params) = &request.parameters {
            if let Some(temp) = params.get("temperature") {
                body["temperature"] = temp.clone();
            }
            if let Some(max_tokens) = params.get("max_tokens") {
                body["max_tokens"] = max_tokens.clone();
            }
            if let Some(top_p) = params.get("top_p") {
                body["top_p"] = top_p.clone();
            }
            if let Some(stop) = params.get("stop") {
                body["stop"] = stop.clone();
            }
        }

        if let Some(tools) = &request.tools {
            let tool_defs: Vec<serde_json::Value> = tools.iter().map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            }).collect();
            body["tools"] = serde_json::json!(tool_defs);
        }

        if request.stream == Some(true) {
            body["stream"] = serde_json::json!(true);
        }

        let mut req_builder = reqwest::Client::new()
            .request(Method::POST, &url)
            .header("Content-Type", "application/json")
            .json(&body);

        if let Some(api_key) = &profile.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        if let Some(headers) = &profile.headers {
            for (key, value) in headers.iter() {
                req_builder = req_builder.header(key, value.as_str().unwrap_or(""));
            }
        }

        req_builder.build().map_err(|e| crate::error::LlmError::HttpError(e))
    }

    fn parse_response(&self, body: &str) -> LlmResult<LlmResponseType> {
        let json: serde_json::Value = serde_json::from_str(body)?;

        let id = json.get("id").and_then(|v| v.as_str()).map(String::from);
        let model = json.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let empty_choices = vec![];
        let choices = json.get("choices").and_then(|v| v.as_array()).unwrap_or(&empty_choices);
        let first_choice = choices.first();

        let content = first_choice
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .map(String::from);

        let finish_reason = first_choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(|r| r.as_str())
            .map(String::from);

        let tool_calls = first_choice
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("tool_calls"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter().filter_map(|tc| {
                    let id = tc.get("id")?.as_str()?.to_string();
                    let function = tc.get("function")?;
                    let name = function.get("name")?.as_str()?.to_string();
                    let arguments = function.get("arguments")?.as_str()?.to_string();
                    Some(wf_types::message::LlmToolCall {
                        id,
                        r#type: "function".to_string(),
                        function: wf_types::message::LlmFunctionCall { name, arguments },
                    })
                }).collect()
            });

        let usage = json.get("usage").map(|u| wf_types::llm::TokenUsageStats {
            prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            completion_tokens: u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            reasoning_tokens: None,
            prompt_tokens_cost: None,
            completion_tokens_cost: None,
            total_cost: None,
        });

        let message = wf_types::message::Message {
            id: wf_types::Id::new(),
            role: wf_types::message::MessageRole::Assistant,
            content: wf_types::message::MessageContentValue::Text(content.clone().unwrap_or_default()),
            timestamp: wf_common::time::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: tool_calls.clone(),
            thinking: None,
            metadata: None,
        };

        Ok(LlmResponseType {
            id,
            model,
            content,
            message,
            tool_calls,
            usage,
            finish_reason,
            duration: 0,
            reasoning_content: None,
            reasoning_tokens: None,
            warnings: None,
        })
    }

    fn parse_stream_chunk(&self, data: &str) -> LlmResult<Option<MessageStreamEvent>> {
        if data == "[DONE]" {
            return Ok(Some(MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {})));
        }

        let json: serde_json::Value = serde_json::from_str(data)?;

        let empty_choices = vec![];
        let choices = json.get("choices").and_then(|v| v.as_array()).unwrap_or(&empty_choices);
        if let Some(choice) = choices.first() {
            if let Some(delta) = choice.get("delta") {
                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                    return Ok(Some(MessageStreamEvent::Text(
                        wf_types::llm::MessageStreamText { text: content.to_string() }
                    )));
                }
            }
        }

        Ok(None)
    }

    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>> {
        let tool_defs: Vec<serde_json::Value> = tools.iter().map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        }).collect();
        Ok(tool_defs)
    }

    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall> {
        result.tool_calls.clone().unwrap_or_default()
    }
}
