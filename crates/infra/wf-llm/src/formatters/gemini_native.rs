use super::LlmFormatter;
use crate::error::LlmResult;
use reqwest::Method;
use std::sync::atomic::{AtomicUsize, Ordering};
use wf_types::llm::{LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent};
use wf_types::tool::Tool;

/// Globally unique tool call indices for Gemini streams (each `functionCall`
/// part is a complete snapshot; unique indices keep separate calls apart in
/// the accumulator).
static GEMINI_CALL_INDEX: AtomicUsize = AtomicUsize::new(0);

pub struct GeminiNativeFormatter {
    base_url: String,
}

impl Default for GeminiNativeFormatter {
    fn default() -> Self {
        Self::new()
    }
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
        messages
            .iter()
            .filter_map(|msg| {
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
                                let args: serde_json::Value = serde_json::from_str(
                                    &tc.function.arguments,
                                )
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
                    wf_types::message::MessageContentValue::Rich(blocks) => blocks
                        .iter()
                        .filter_map(|block| match block {
                            wf_types::message::MessageContent::Text { text } => {
                                Some(serde_json::json!({"text": text}))
                            }
                            wf_types::message::MessageContent::ToolResult { tool_result } => {
                                let content_val: serde_json::Value =
                                    serde_json::from_str(&tool_result.content).unwrap_or_else(
                                        |_| serde_json::Value::String(tool_result.content.clone()),
                                    );
                                Some(serde_json::json!({
                                    "function_response": {
                                        "name": "",
                                        "response": content_val,
                                    }
                                }))
                            }
                            _ => None,
                        })
                        .collect(),
                };

                Some(serde_json::json!({"role": role, "parts": parts}))
            })
            .collect()
    }

    fn convert_generation_config(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> serde_json::Value {
        let merged_params =
            crate::formatter_helpers::merge_parameters(profile, &request.parameters);

        // Defaults match the deprecated formatter.
        let mut config = serde_json::json!({
            "temperature": merged_params.get("temperature").cloned().unwrap_or_else(|| serde_json::json!(0.7)),
            "maxOutputTokens": merged_params.get("max_tokens").cloned().unwrap_or_else(|| serde_json::json!(4096)),
            "topP": merged_params.get("top_p").cloned().unwrap_or_else(|| serde_json::json!(1.0)),
            "topK": merged_params.get("top_k").cloned().unwrap_or_else(|| serde_json::json!(40)),
        });

        if let Some(stop) = merged_params.get("stop") {
            let stop_seqs: Vec<String> = stop
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            config["stopSequences"] = serde_json::json!(stop_seqs);
        }

        config
    }
}

impl LlmFormatter for GeminiNativeFormatter {
    fn build_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<reqwest::Request> {
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            profile.base_url.as_deref().unwrap_or(&self.base_url),
            profile.model,
            profile.api_key.as_deref().unwrap_or("")
        );

        let body = self.build_body(request, profile)?;

        let mut req_builder = reqwest::Client::new()
            .request(Method::POST, &url)
            .header("Content-Type", "application/json")
            .json(&body);

        req_builder = super::shared::apply_auth_and_headers(req_builder, profile, "native");

        req_builder
            .build()
            .map_err(crate::error::LlmError::HttpError)
    }

    fn parse_response(&self, body: &str, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        let mut result = self.parse_gemini_response(body)?;
        if super::shared::is_text_mode(request) {
            if let Some(content) = result.content.clone() {
                let calls = super::shared::parse_text_tool_calls(request, &content);
                if !calls.is_empty() {
                    result.tool_calls = Some(calls.clone());
                    result.message.tool_calls = Some(calls);
                }
            }
        }
        Ok(result)
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
                                    wf_types::llm::MessageStreamText {
                                        snapshot: String::new(),
                                        text: text.to_string(),
                                    },
                                )));
                            }
                            if let Some(thought) = part.get("thought").and_then(|v| v.as_str()) {
                                return Ok(Some(MessageStreamEvent::ReasoningText(
                                    wf_types::llm::MessageStreamReasoning {
                                        snapshot: String::new(),
                                        reasoning: thought.to_string(),
                                    },
                                )));
                            }
                            if let Some(func_call) = part.get("functionCall") {
                                let name = func_call
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let args = func_call
                                    .get("args")
                                    .cloned()
                                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                                let arguments = serde_json::to_string(&args).unwrap_or_default();
                                return Ok(Some(MessageStreamEvent::ToolCallDelta(
                                    wf_types::llm::MessageStreamToolCallDelta {
                                        index: GEMINI_CALL_INDEX.fetch_add(1, Ordering::Relaxed),
                                        id: None,
                                        name: Some(name),
                                        arguments: Some(arguments),
                                        is_snapshot: true,
                                    },
                                )));
                            }
                        }
                    }
                }
                if candidate.get("finishReason").is_some() {
                    return Ok(Some(MessageStreamEvent::End(
                        wf_types::llm::MessageStreamEnd {},
                    )));
                }
            }
        }

        Ok(None)
    }

    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>> {
        let function_declarations: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();
        Ok(function_declarations)
    }

    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall> {
        result.tool_calls.clone().unwrap_or_default()
    }
}

impl GeminiNativeFormatter {
    /// Build the request body (separated from the HTTP layer for testability).
    fn build_body(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<serde_json::Value> {
        let generation_config = self.convert_generation_config(request, profile);

        let use_text_mode = super::shared::is_text_mode(request);

        // System instructions are sent in the dedicated `systemInstruction`
        // field in both modes. Text mode injects the original system + tool
        // usage instructions + declarations; native mode keeps the original
        // system message.
        let (system_content, _) = crate::tool_format::extract_system_message(&request.messages);

        let history = if use_text_mode {
            super::shared::convert_history_for_text_mode(&request.messages, request)
        } else {
            request.messages.clone()
        };
        let messages = self.convert_messages(&history);

        let mut body = serde_json::json!({
            "contents": messages,
            "generationConfig": generation_config,
        });

        if use_text_mode {
            let system = super::shared::text_mode_system_content(request);
            if !system.is_empty() {
                body["systemInstruction"] = serde_json::json!({"parts": [{"text": system}]});
            }
        } else {
            if let Some(system) = system_content {
                if !system.is_empty() {
                    body["systemInstruction"] = serde_json::json!({"parts": [{"text": system}]});
                }
            }
            if let Some(tools) = &request.tools {
                let function_declarations: Vec<serde_json::Value> = tools
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        })
                    })
                    .collect();
                body["tools"] =
                    serde_json::json!([{"functionDeclarations": function_declarations}]);
            }
        }

        super::shared::apply_custom_body(&mut body, profile);

        Ok(body)
    }

    fn parse_gemini_response(&self, body: &str) -> LlmResult<LlmResponseType> {
        let json: serde_json::Value = serde_json::from_str(body)?;

        let candidates = json.get("candidates").and_then(|v| v.as_array());
        let first_candidate = candidates.and_then(|c| c.first());

        let mut text_content = String::new();
        let mut reasoning_content: Option<String> = None;
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
                        if let Some(thought) = part.get("thought").and_then(|v| v.as_str()) {
                            reasoning_content
                                .get_or_insert_with(String::new)
                                .push_str(thought);
                        }
                        if let Some(func_call) = part.get("functionCall") {
                            let name = func_call
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let args = func_call
                                .get("args")
                                .cloned()
                                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
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

        let usage = json
            .get("usageMetadata")
            .map(|u| wf_types::llm::TokenUsageStats {
                prompt_tokens: u
                    .get("promptTokenCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                completion_tokens: u
                    .get("candidatesTokenCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                total_tokens: u
                    .get("totalTokenCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                reasoning_tokens: None,
                cache_read_tokens: u
                    .get("cachedContentTokenCount")
                    .and_then(|v| v.as_u64())
                    .map(|r| r as u32),
                cache_write_tokens: None,
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
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls.clone())
            },
            thinking: None,
            metadata: None,
        };

        let finish_reason = first_candidate
            .and_then(|c| c.get("finishReason"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let mut metadata = std::collections::HashMap::new();
        if let Some(fr) = &finish_reason {
            metadata.insert("finish_reason".to_string(), serde_json::json!(fr));
        }
        let metadata = if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        };

        Ok(LlmResponseType {
            id: Some(wf_common::generate_id()),
            model: json
                .get("modelVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            content: if text_content.is_empty() {
                None
            } else {
                Some(text_content)
            },
            message,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            usage,
            finish_reason,
            duration: 0,
            reasoning_content: reasoning_content.clone(),
            reasoning_tokens: None,
            metadata,
            stream_stats: None,
            warnings: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_call_part_emits_snapshot_delta() {
        let chunk = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_weather","args":{"city":"Beijing"}}}]},"finishReason":"STOP"}]}"#;
        match GeminiNativeFormatter::new()
            .parse_stream_chunk(chunk)
            .expect("chunk must parse")
        {
            Some(MessageStreamEvent::ToolCallDelta(delta)) => {
                assert_eq!(delta.name.as_deref(), Some("get_weather"));
                let args: serde_json::Value =
                    serde_json::from_str(delta.arguments.as_deref().unwrap()).unwrap();
                assert_eq!(args["city"], "Beijing");
                assert!(delta.is_snapshot);
            }
            other => panic!("expected ToolCallDelta, got {:?}", other),
        }
    }

    fn profile() -> LlmProfile {
        LlmProfile {
            id: "p1".to_string(),
            name: "test".to_string(),
            provider: wf_types::llm::LlmProvider::GeminiNative,
            model: "gemini-1.5-pro".to_string(),
            api_key: Some("sk-test".to_string()),
            base_url: None,
            parameters: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_format: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
        }
    }

    fn msg(role: wf_types::message::MessageRole, text: &str) -> wf_types::message::Message {
        wf_types::message::Message {
            id: wf_types::Id::new(),
            role,
            content: wf_types::message::MessageContentValue::Text(text.to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn request(
        messages: Vec<wf_types::message::Message>,
        params: Option<serde_json::Value>,
    ) -> LlmRequest {
        LlmRequest {
            profile_id: "p1".to_string(),
            messages,
            parameters: params,
            tools: None,
            tool_call_format: None,
            locked_tool_call_format: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        }
    }

    #[test]
    fn native_mode_sends_system_instruction() {
        let formatter = GeminiNativeFormatter::new();
        let req = request(
            vec![
                msg(wf_types::message::MessageRole::System, "You are a helper"),
                msg(wf_types::message::MessageRole::User, "Hello"),
            ],
            None,
        );
        let body = formatter.build_body(&req, &profile()).expect("must build");

        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            serde_json::json!("You are a helper")
        );
        let roles: Vec<&str> = body["contents"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["role"].as_str())
            .collect();
        assert_eq!(roles, vec!["user"], "system must not appear in contents");
    }

    #[test]
    fn native_mode_sends_tools_alongside_system() {
        let formatter = GeminiNativeFormatter::new();
        let req = request(
            vec![
                msg(wf_types::message::MessageRole::System, "You are a helper"),
                msg(wf_types::message::MessageRole::User, "Hello"),
            ],
            None,
        );
        let mut req = req;
        req.tools = Some(vec![serde_json::from_value(serde_json::json!({
            "id": wf_types::Id::new(),
            "name": "get_weather",
            "description": "Get weather",
            "tool_type": "built_in",
            "parameters": {"type": "object", "properties": {}, "required": []},
            "enabled": true
        }))
        .unwrap()]);
        let body = formatter.build_body(&req, &profile()).expect("must build");

        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            serde_json::json!("You are a helper")
        );
        assert_eq!(
            body["tools"][0]["functionDeclarations"][0]["name"],
            serde_json::json!("get_weather")
        );
    }

    #[test]
    fn generation_config_defaults_and_overrides() {
        let formatter = GeminiNativeFormatter::new();
        let body = formatter
            .build_body(&request(vec![], None), &profile())
            .expect("must build");

        assert_eq!(
            body["generationConfig"]["temperature"],
            serde_json::json!(0.7)
        );
        assert_eq!(
            body["generationConfig"]["maxOutputTokens"],
            serde_json::json!(4096)
        );
        assert_eq!(body["generationConfig"]["topP"], serde_json::json!(1.0));
        assert_eq!(body["generationConfig"]["topK"], serde_json::json!(40));

        let req = request(
            vec![],
            Some(serde_json::json!({"temperature": 0.2, "max_tokens": 128})),
        );
        let body = formatter.build_body(&req, &profile()).expect("must build");
        assert_eq!(
            body["generationConfig"]["temperature"],
            serde_json::json!(0.2)
        );
        assert_eq!(
            body["generationConfig"]["maxOutputTokens"],
            serde_json::json!(128)
        );
    }
}
