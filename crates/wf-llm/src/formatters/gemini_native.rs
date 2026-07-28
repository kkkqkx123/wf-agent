use async_trait::async_trait;
use reqwest::Method;
use wf_types::llm::{LlmRequest, LlmResult as LlmResponseType, LlmProfile, MessageStreamEvent};
use crate::error::LlmResult;
use wf_types::tool::Tool;
use super::LlmFormatter;

pub struct GeminiNativeFormatter {
    base_url: String,
}

impl GeminiNativeFormatter {
    pub fn new() -> Self {
        Self {
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
        }
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self { base_url }
    }

    fn convert_messages(&self, messages: &[wf_types::message::Message]) -> Vec<serde_json::Value> {
        messages.iter().filter_map(|msg| {
            let role = match msg.role {
                wf_types::message::MessageRole::System => return None,
                wf_types::message::MessageRole::User => "user",
                wf_types::message::MessageRole::Assistant => "model",
                wf_types::message::MessageRole::Tool => "function",
            };

            let parts = match &msg.content {
                wf_types::message::MessageContentValue::Text(text) => {
                    if text.is_empty() && msg.tool_calls.is_none() {
                        return None;
                    }
                    let mut p = Vec::new();
                    if !text.is_empty() {
                        p.push(serde_json::json!({"text": text}));
                    }
                    if let Some(ref tool_calls) = msg.tool_calls {
                        for tc in tool_calls {
                            let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                            p.push(serde_json::json!({
                                "function_call": {
                                    "name": tc.function.name,
                                    "args": args,
                                }
                            }));
                        }
                    }
                    p
                }
                wf_types::message::MessageContentValue::Rich(blocks) => {
                    blocks.iter().filter_map(|block| {
                        match block {
                            wf_types::message::MessageContent::Text { text } => {
                                Some(serde_json::json!({"text": text}))
                            }
                            wf_types::message::MessageContent::ToolResult { tool_result } => {
                                let content_val: serde_json::Value = serde_json::from_str(&tool_result.content)
                                    .unwrap_or_else(|_| serde_json::Value::String(tool_result.content.clone()));
                                Some(serde_json::json!({
                                    "function_response": {
                                        "name": "",
                                        "response": content_val,
                                    }
                                }))
                            }
                            _ => None,
                        }
                    }).collect()
                }
            };

            Some(serde_json::json!({"role": role, "parts": parts}))
        }).collect()
    }

    fn convert_generation_config(&self, request: &LlmRequest) -> serde_json::Value {
        let mut config = serde_json::json!({});

        if let Some(params) = &request.parameters {
            if let Some(temp) = params.get("temperature") {
                config["temperature"] = temp.clone();
            }
            if let Some(max_tokens) = params.get("max_tokens") {
                config["maxOutputTokens"] = max_tokens.clone();
            }
            if let Some(top_p) = params.get("top_p") {
                config["topP"] = top_p.clone();
            }
            if let Some(top_k) = params.get("top_k") {
                config["topK"] = top_k.clone();
            }
            if let Some(stop) = params.get("stop") {
                let stop_seqs: Vec<String> = stop.as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                config["stopSequences"] = serde_json::json!(stop_seqs);
            }
        }

        config
    }
}

#[async_trait]
impl LlmFormatter for GeminiNativeFormatter {
    fn build_request(&self, request: &LlmRequest, profile: &LlmProfile) -> LlmResult<reqwest::Request> {
        let url = format!("{}/models/{}:generateContent?key={}",
            profile.base_url.as_deref().unwrap_or(&self.base_url),
            profile.model,
            profile.api_key.as_deref().unwrap_or("")
        );

        let messages = self.convert_messages(&request.messages);
        let generation_config = self.convert_generation_config(request);

        let mut body = serde_json::json!({
            "contents": messages,
            "generationConfig": generation_config,
        });

        if let Some(tools) = &request.tools {
            let function_declarations: Vec<serde_json::Value> = tools.iter().map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            }).collect();
            body["tools"] = serde_json::json!([{"functionDeclarations": function_declarations}]);
        }

        let mut req_builder = reqwest::Client::new()
            .request(Method::POST, &url)
            .header("Content-Type", "application/json")
            .json(&body);

        if let Some(headers) = &profile.headers {
            for (key, value) in headers.iter() {
                req_builder = req_builder.header(key, value.as_str().unwrap_or(""));
            }
        }

        req_builder.build().map_err(|e| crate::error::LlmError::HttpError(e))
    }

    fn parse_response(&self, body: &str) -> LlmResult<LlmResponseType> {
        let json: serde_json::Value = serde_json::from_str(body)?;

        let candidates = json.get("candidates").and_then(|v| v.as_array());
        let first_candidate = candidates.and_then(|c| c.first());

        let mut text_content = String::new();
        let mut tool_calls = Vec::new();

        if let Some(candidate) = first_candidate {
            let content = candidate.get("content");
            if let Some(c) = content {
                let parts = c.get("parts").and_then(|v| v.as_array());
                if let Some(parts) = parts {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                            text_content.push_str(text);
                        }
                        if let Some(func_call) = part.get("functionCall") {
                            let name = func_call.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let args = func_call.get("args").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                            let arguments = serde_json::to_string(&args).unwrap_or_default();
                            tool_calls.push(wf_types::message::LlmToolCall {
                                id: format!("gemini_call_{}", wf_common::generate_id()),
                                r#type: "function".to_string(),
                                function: wf_types::message::LlmFunctionCall { name, arguments },
                            });
                        }
                    }
                }
            }
        }

        let usage = json.get("usageMetadata").map(|u| wf_types::llm::TokenUsageStats {
            prompt_tokens: u.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            completion_tokens: u.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            total_tokens: u.get("totalTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            reasoning_tokens: None,
            prompt_tokens_cost: None,
            completion_tokens_cost: None,
            total_cost: None,
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

        Ok(LlmResponseType {
            id: Some(wf_common::generate_id()),
            model: json.get("modelVersion").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            content: if text_content.is_empty() { None } else { Some(text_content) },
            message,
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            usage,
            finish_reason: first_candidate.and_then(|c| c.get("finishReason")).and_then(|v| v.as_str()).map(String::from),
            duration: 0,
            reasoning_content: None,
            reasoning_tokens: None,
            warnings: None,
        })
    }

    fn parse_stream_chunk(&self, data: &str) -> LlmResult<Option<MessageStreamEvent>> {
        if data.is_empty() {
            return Ok(None);
        }

        let json: serde_json::Value = serde_json::from_str(data)?;

        if let Some(candidates) = json.get("candidates").and_then(|v| v.as_array()) {
            if let Some(candidate) = candidates.first() {
                if let Some(content) = candidate.get("content") {
                    if let Some(parts) = content.get("parts").and_then(|v| v.as_array()) {
                        for part in parts {
                            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                                return Ok(Some(MessageStreamEvent::Text(
                                    wf_types::llm::MessageStreamText { text: text.to_string() }
                                )));
                            }
                        }
                    }
                }
                if candidate.get("finishReason").is_some() {
                    return Ok(Some(MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {})));
                }
            }
        }

        Ok(None)
    }

    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>> {
        let function_declarations: Vec<serde_json::Value> = tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        }).collect();
        Ok(function_declarations)
    }

    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall> {
        result.tool_calls.clone().unwrap_or_default()
    }
}
