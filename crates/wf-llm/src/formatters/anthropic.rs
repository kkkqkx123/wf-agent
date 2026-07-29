use reqwest::Method;
use wf_types::llm::{LlmRequest, LlmResult as LlmResponseType, LlmProfile, MessageStreamEvent};
use crate::error::LlmResult;
use wf_types::tool::Tool;
use super::LlmFormatter;

pub struct AnthropicFormatter {
    base_url: String,
}

impl Default for AnthropicFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl AnthropicFormatter {
    pub fn new() -> Self {
        Self {
            base_url: "https://api.anthropic.com/v1".to_string(),
        }
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self { base_url }
    }

    fn convert_messages(&self, messages: &[wf_types::message::Message]) -> Vec<serde_json::Value> {
        let mut result = Vec::new();
        let mut current_text: Option<String> = None;

        for msg in messages {
            let role = match msg.role {
                wf_types::message::MessageRole::System => continue,
                wf_types::message::MessageRole::User => "user",
                wf_types::message::MessageRole::Assistant => "assistant",
                wf_types::message::MessageRole::Tool => "user",
            };

            let content = match &msg.content {
                wf_types::message::MessageContentValue::Text(text) => {
                    if text.is_empty() { continue; }
                    serde_json::json!([{"type": "text", "text": text}])
                }
                wf_types::message::MessageContentValue::Rich(blocks) => {
                    let converted: Vec<serde_json::Value> = blocks.iter().map(|block| {
                        match block {
                            wf_types::message::MessageContent::Text { text } => {
                                serde_json::json!({"type": "text", "text": text})
                            }
                            wf_types::message::MessageContent::ToolUse { tool_use } => {
                                serde_json::json!({
                                    "type": "tool_use",
                                    "id": tool_use.id,
                                    "name": tool_use.name,
                                    "input": tool_use.input,
                                })
                            }
                            wf_types::message::MessageContent::ToolResult { tool_result } => {
                                serde_json::json!({
                                    "type": "tool_result",
                                    "tool_use_id": tool_result.tool_use_id,
                                    "content": tool_result.content,
                                    "is_error": tool_result.is_error.unwrap_or(false),
                                })
                            }
                            wf_types::message::MessageContent::ImageUrl { image_url } => {
                                let url = &image_url.url;
                                let (media_type, data) = if let Some(rest) = url.strip_prefix("data:") {
                                    if let Some(semicolon) = rest.find(';') {
                                        let mime = &rest[..semicolon];
                                        let b64 = rest[semicolon + 1..].strip_prefix("base64,").unwrap_or("").to_string();
                                        (mime.to_string(), b64)
                                    } else {
                                        ("image/png".to_string(), rest.to_string())
                                    }
                                } else {
                                    ("image/png".to_string(), url.clone())
                                };
                                serde_json::json!({
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": media_type,
                                        "data": data,
                                    }
                                })
                            }
                            _ => serde_json::json!({"type": "text", "text": ""}),
                        }
                    }).collect();
                    serde_json::json!(converted)
                }
            };

            if role == "user" {
                if let Some(tool_id) = &msg.tool_call_id {
                    let entry = serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_id,
                            "content": match &msg.content {
                                wf_types::message::MessageContentValue::Text(t) => t.clone(),
                                _ => "".to_string(),
                            }
                        }]
                    });
                    result.push(entry);
                    continue;
                }
                if let Some(text) = &current_text {
                    let entry = serde_json::json!({
                        "role": "assistant",
                        "content": [{"type": "text", "text": text}]
                    });
                    result.push(entry);
                    current_text = None;
                }
            } else if role == "assistant" {
                if let Some(text) = &current_text {
                    let entry = serde_json::json!({
                        "role": "assistant",
                        "content": [{"type": "text", "text": text}]
                    });
                    result.push(entry);
                    current_text = None;
                }
                if let Some(ref tool_calls) = msg.tool_calls {
                    let mut blocks = Vec::new();
                    if let wf_types::message::MessageContentValue::Text(text) = &msg.content {
                        if !text.is_empty() {
                            blocks.push(serde_json::json!({"type": "text", "text": text}));
                        }
                    }
                    for tc in tool_calls {
                        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        blocks.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.function.name,
                            "input": args,
                        }));
                    }
                    result.push(serde_json::json!({"role": "assistant", "content": blocks}));
                    continue;
                }
            }

            result.push(serde_json::json!({"role": role, "content": content}));
        }

        if let Some(text) = current_text {
            result.push(serde_json::json!({"role": "assistant", "content": [{"type": "text", "text": text}]}));
        }

        result
    }
}

impl LlmFormatter for AnthropicFormatter {
    fn build_request(&self, request: &LlmRequest, profile: &LlmProfile) -> LlmResult<reqwest::Request> {
        let url = format!("{}/messages", profile.base_url.as_deref().unwrap_or(&self.base_url));

        let messages = self.convert_messages(&request.messages);

        let mut body = serde_json::json!({
            "model": profile.model,
            "messages": messages,
            "max_tokens": 4096,
        });

        let merged_params = crate::formatter_helpers::merge_parameters(profile, &request.parameters);
        if let Some(temp) = merged_params.get("temperature") {
            body["temperature"] = temp.clone();
        }
        if let Some(max_tokens) = merged_params.get("max_tokens") {
            body["max_tokens"] = max_tokens.clone();
        }
        if let Some(top_p) = merged_params.get("top_p") {
            body["top_p"] = top_p.clone();
        }
        if let Some(stop) = merged_params.get("stop") {
            body["stop_sequences"] = stop.clone();
        }

        if let Some(tools) = &request.tools {
            let tool_defs: Vec<serde_json::Value> = tools.iter().map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
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
            .header("anthropic-version", "2023-06-01")
            .json(&body);

        if let Some(api_key) = &profile.api_key {
            req_builder = req_builder.header("x-api-key", api_key);
        }

        if let Some(headers) = &profile.headers {
            for (key, value) in headers.iter() {
                req_builder = req_builder.header(key, value.as_str().unwrap_or(""));
            }
        }

        req_builder.build().map_err(crate::error::LlmError::HttpError)
    }

    fn parse_response(&self, body: &str) -> LlmResult<LlmResponseType> {
        let json: serde_json::Value = serde_json::from_str(body)?;

        let id = json.get("id").and_then(|v| v.as_str()).map(String::from);
        let model = json.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let stop_reason = json.get("stop_reason").and_then(|v| v.as_str()).map(String::from);

        let content_arr = json.get("content").and_then(|v| v.as_array());
        let mut text_content = String::new();
        let mut tool_calls = Vec::new();

        if let Some(blocks) = content_arr {
            for block in blocks {
                let block_type = block.get("type").and_then(|v| v.as_str());
                match block_type {
                    Some("text") => {
                        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                            text_content.push_str(text);
                        }
                    }
                    Some("tool_use") => {
                        let tc_id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let input = block.get("input").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        let arguments = serde_json::to_string(&input).unwrap_or_default();
                        tool_calls.push(wf_types::message::LlmToolCall {
                            id: tc_id,
                            r#type: "function".to_string(),
                            function: wf_types::message::LlmFunctionCall { name, arguments },
                        });
                    }
                    _ => {}
                }
            }
        }

        let usage = json.get("usage").map(|u| {
            let input = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let output = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            wf_types::llm::TokenUsageStats {
                prompt_tokens: input,
                completion_tokens: output,
                total_tokens: input + output,
                reasoning_tokens: None,
                prompt_tokens_cost: None,
                completion_tokens_cost: None,
                total_cost: None,
            }
        });

        let message = wf_types::message::Message {
            id: wf_types::Id::new(),
            role: wf_types::message::MessageRole::Assistant,
            content: wf_types::message::MessageContentValue::Text(text_content.clone()),
            timestamp: wf_common::time::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls.clone()) },
            thinking: None,
            metadata: None,
        };

        let reasoning_token_count = json.get("usage")
            .and_then(|u| u.get("completion_tokens_details"))
            .and_then(|d| d.get("reasoning_tokens"))
            .and_then(|r| r.as_u64());

        let mut metadata = std::collections::HashMap::new();
        if let Some(t) = json.get("type").and_then(|v| v.as_str()) {
            metadata.insert("type".to_string(), serde_json::json!(t));
        }
        if let Some(sr) = &stop_reason {
            metadata.insert("stop_reason".to_string(), serde_json::json!(sr));
        }
        let metadata = if metadata.is_empty() { None } else { Some(metadata) };

        Ok(LlmResponseType {
            id,
            model,
            content: if text_content.is_empty() { None } else { Some(text_content) },
            message,
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            usage,
            finish_reason: stop_reason,
            duration: 0,
            reasoning_content: None,
            reasoning_tokens: reasoning_token_count.map(|r| r as u32),
            metadata,
            stream_stats: None,
            warnings: None,
        })
    }

    fn parse_stream_chunk(&self, data: &str) -> LlmResult<Option<MessageStreamEvent>> {
        if data.is_empty() {
            return Ok(None);
        }

        let json: serde_json::Value = serde_json::from_str(data)?;
        let event_type = json.get("type").and_then(|v| v.as_str());

        match event_type {
            Some("message_start") => Ok(None),
            Some("content_block_start") => {
                let block = json.get("content_block");
                if let Some(b) = block {
                    if b.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                        return Ok(None);
                    }
                }
                Ok(None)
            }
            Some("content_block_delta") => {
                let delta = json.get("delta");
                if let Some(d) = delta {
                    match d.get("type").and_then(|v| v.as_str()) {
                        Some("text_delta") => {
                            if let Some(text) = d.get("text").and_then(|v| v.as_str()) {
                                return Ok(Some(MessageStreamEvent::Text(
                                    wf_types::llm::MessageStreamText { text: text.to_string() }
                                )));
                            }
                        }
                        Some("thinking_delta") => {
                            if let Some(reasoning) = d.get("thinking").and_then(|v| v.as_str()) {
                                return Ok(Some(MessageStreamEvent::ReasoningText(
                                    wf_types::llm::MessageStreamReasoning { reasoning: reasoning.to_string() }
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Ok(None)
            }
            Some("message_delta") => {
                let delta = json.get("delta");
                if let Some(d) = delta {
                    if let Some(stop_reason) = d.get("stop_reason").and_then(|v| v.as_str()) {
                        let is_done = matches!(stop_reason, "end_turn" | "max_tokens" | "stop_sequence" | "tool_use");
                        if is_done {
                            return Ok(Some(MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {})));
                        }
                    }
                }
                Ok(None)
            }
            Some("message_stop") => Ok(Some(MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}))),
            _ => Ok(None),
        }
    }

    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>> {
        let tool_defs: Vec<serde_json::Value> = tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            })
        }).collect();
        Ok(tool_defs)
    }

    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall> {
        result.tool_calls.clone().unwrap_or_default()
    }
}
