use crate::error::LlmResult;
use wf_types::llm::{
    LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent, ToolCallFormat,
};
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};

/// Whether the request runs in text-based tool mode (non-native tool format).
pub fn is_text_mode(request: &LlmRequest) -> bool {
    matches!(
        request.tool_call_format,
        Some(ref f) if *f != ToolCallFormat::Native
    )
}

/// Effective tool call format: the locked format wins, otherwise the request
/// format, otherwise native.
pub fn effective_tool_call_format(request: &LlmRequest) -> ToolCallFormat {
    request
        .locked_tool_call_format
        .as_ref()
        .map(|c| c.format.clone())
        .or_else(|| request.tool_call_format.clone())
        .unwrap_or(ToolCallFormat::Native)
}

/// Convert message history to the text-based tool format when the request runs
/// in text mode (native tool calls / results become XML or JSON blocks in the
/// message content). Messages are returned unchanged in native mode.
pub fn convert_history_for_text_mode(messages: &[Message], request: &LlmRequest) -> Vec<Message> {
    if !is_text_mode(request) {
        return messages.to_vec();
    }
    let format = effective_tool_call_format(request);
    let markers = request
        .locked_tool_call_format
        .as_ref()
        .and_then(|c| c.markers.clone());
    crate::messaging::history_converter::convert_to_text_mode(messages, &format, markers.as_ref())
}

/// Build the system prompt content for text-based tool mode: existing system
/// message + tool usage instructions + tool declarations.
pub fn text_mode_system_content(request: &LlmRequest) -> String {
    use crate::tool_format::{build_text_mode_system_content, extract_system_message};
    let format = effective_tool_call_format(request);
    let (system, _) = extract_system_message(&request.messages);
    let tools = request.tools.as_deref().unwrap_or(&[]);
    build_text_mode_system_content(system.as_deref().unwrap_or(""), tools, format, false)
}

/// Parse tool calls from text content in text-based tool mode.
pub fn parse_text_tool_calls(
    request: &LlmRequest,
    content: &str,
) -> Vec<wf_types::message::LlmToolCall> {
    use crate::tool_call_parser::parse_from_text;
    use crate::tool_format::get_tool_call_parser_options;
    let format = effective_tool_call_format(request);
    if format == ToolCallFormat::Native {
        return Vec::new();
    }
    let markers = request
        .locked_tool_call_format
        .as_ref()
        .and_then(|c| c.markers.clone());
    let options = get_tool_call_parser_options(format, markers.as_ref());
    parse_from_text(content, &options)
}

pub fn convert_openai_messages(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };

            let mut entry = match &msg.content {
                MessageContentValue::Text(text) => {
                    serde_json::json!({"role": role, "content": text})
                }
                MessageContentValue::Rich(blocks) => {
                    let content: Vec<serde_json::Value> = blocks
                        .iter()
                        .map(|block| match block {
                            MessageContent::Text { text } => {
                                serde_json::json!({"type": "text", "text": text})
                            }
                            MessageContent::ImageUrl { image_url } => {
                                let mut img = serde_json::json!({
                                    "type": "image_url",
                                    "image_url": {"url": image_url.url}
                                });
                                if let Some(ref detail) = image_url.detail {
                                    img["image_url"]["detail"] = serde_json::json!(detail);
                                }
                                img
                            }
                            MessageContent::ToolUse { tool_use } => {
                                serde_json::json!({
                                    "type": "tool_use",
                                    "id": tool_use.id,
                                    "name": tool_use.name,
                                    "input": tool_use.input,
                                })
                            }
                            MessageContent::ToolResult { tool_result } => {
                                serde_json::json!({
                                    "type": "tool_result",
                                    "tool_use_id": tool_result.tool_use_id,
                                    "content": tool_result.content,
                                    "is_error": tool_result.is_error.unwrap_or(false),
                                })
                            }
                            MessageContent::Thinking { thinking, .. } => {
                                serde_json::json!({"type": "text", "text": thinking})
                            }
                        })
                        .collect();
                    serde_json::json!({"role": role, "content": content})
                }
            };

            if let Some(ref tool_calls) = msg.tool_calls {
                let calls: Vec<serde_json::Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": tc.r#type,
                            "function": {
                                "name": tc.function.name,
                                "arguments": tc.function.arguments,
                            }
                        })
                    })
                    .collect();
                entry["tool_calls"] = serde_json::json!(calls);
            }

            if let Some(ref tool_call_id) = msg.tool_call_id {
                entry["tool_call_id"] = serde_json::json!(tool_call_id);
            }

            entry
        })
        .collect()
}

/// Merge profile-level and request-level parameters into the body.
///
/// Everything is passed through as-is (matching the deprecated TS
/// `Object.assign(body, otherParams)` behavior) except the `stream` key,
/// which is controlled by the caller. This lets provider-specific options
/// like `response_format`, `reasoning_effort`, `frequency_penalty`,
/// `thinkingConfig` etc. flow to the API untouched.
pub fn merge_and_apply_params(
    body: &mut serde_json::Value,
    profile: &LlmProfile,
    request_params: &Option<serde_json::Value>,
) {
    let merged_params = crate::formatter_helpers::merge_parameters(profile, request_params);
    for (key, value) in merged_params {
        if key != "stream" {
            body[key] = value;
        }
    }
}

/// Parse an OpenAI-style usage object into token usage stats.
pub fn parse_openai_usage(u: &serde_json::Value) -> wf_types::llm::TokenUsageStats {
    let reasoning_tokens = u
        .get("completion_tokens_details")
        .and_then(|d| d.get("reasoning_tokens"))
        .and_then(|r| r.as_u64())
        .map(|r| r as u32);
    let cache_read_tokens = u
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|r| r.as_u64())
        .map(|r| r as u32);
    wf_types::llm::TokenUsageStats {
        prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        completion_tokens: u
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        reasoning_tokens,
        cache_read_tokens,
        cache_write_tokens: None,
        prompt_tokens_cost: None,
        completion_tokens_cost: None,
        total_cost: None,
    }
}

pub fn parse_openai_chat_response(body: &str) -> LlmResult<LlmResponseType> {
    let json: serde_json::Value = serde_json::from_str(body)?;

    let id = json.get("id").and_then(|v| v.as_str()).map(String::from);
    let model = json
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let empty_choices = vec![];
    let choices = json
        .get("choices")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_choices);
    let first_choice = choices.first();
    let message = first_choice.and_then(|c| c.get("message"));

    let content = message
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(String::from);

    let finish_reason = first_choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|r| r.as_str())
        .map(String::from);

    let reasoning_content = message
        .and_then(|m| m.get("reasoning_content"))
        .and_then(|r| r.as_str())
        .map(String::from);

    let tool_calls = message
        .and_then(|m| m.get("tool_calls"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    let id = tc.get("id")?.as_str()?.to_string();
                    let function = tc.get("function")?;
                    let name = function.get("name")?.as_str()?.to_string();
                    let arguments = function.get("arguments")?.as_str()?.to_string();
                    Some(wf_types::message::LlmToolCall {
                        id,
                        r#type: "function".to_string(),
                        function: wf_types::message::LlmFunctionCall { name, arguments },
                    })
                })
                .collect()
        });

    let usage = json.get("usage").map(parse_openai_usage);

    let reasoning_tokens = usage.as_ref().and_then(|u| u.reasoning_tokens);

    let msg = wf_types::message::Message {
        id: wf_types::Id::new(),
        role: MessageRole::Assistant,
        content: MessageContentValue::Text(content.clone().unwrap_or_default()),
        timestamp: wf_common::time::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: tool_calls.clone(),
        thinking: reasoning_content.clone(),
        metadata: None,
    };

    let mut metadata = std::collections::HashMap::new();
    if let Some(created) = json.get("created").and_then(|v| v.as_i64()) {
        metadata.insert("created".to_string(), serde_json::json!(created));
    }
    if let Some(fp) = json.get("system_fingerprint").and_then(|v| v.as_str()) {
        metadata.insert("system_fingerprint".to_string(), serde_json::json!(fp));
    }
    let metadata = if metadata.is_empty() {
        None
    } else {
        Some(metadata)
    };

    Ok(LlmResponseType {
        id,
        model,
        content,
        message: msg,
        tool_calls,
        usage,
        finish_reason,
        duration: 0,
        reasoning_content,
        reasoning_tokens,
        metadata,
        stream_stats: None,
        warnings: None,
    })
}

pub fn parse_openai_stream_chunk(data: &str) -> LlmResult<Option<MessageStreamEvent>> {
    if data == "[DONE]" {
        return Ok(Some(MessageStreamEvent::End(
            wf_types::llm::MessageStreamEnd {},
        )));
    }

    let json: serde_json::Value = serde_json::from_str(data)?;

    let empty_choices = vec![];
    let choices = json
        .get("choices")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_choices);

    // A chunk without choices carries only usage (stream_options.include_usage).
    if choices.is_empty() {
        if let Some(usage) = json.get("usage") {
            return Ok(Some(MessageStreamEvent::Usage(
                wf_types::llm::MessageStreamUsage {
                    usage: parse_openai_usage(usage),
                },
            )));
        }
        return Ok(None);
    }

    if let Some(choice) = choices.first() {
        if let Some(delta) = choice.get("delta") {
            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                return Ok(Some(MessageStreamEvent::Text(
                    wf_types::llm::MessageStreamText {
                        snapshot: String::new(),
                        text: content.to_string(),
                    },
                )));
            }
            if let Some(reasoning) = delta.get("reasoning_content").and_then(|r| r.as_str()) {
                return Ok(Some(MessageStreamEvent::ReasoningText(
                    wf_types::llm::MessageStreamReasoning {
                        snapshot: String::new(),
                        reasoning: reasoning.to_string(),
                    },
                )));
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                if let Some(call) = tool_calls.first() {
                    let index = call.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let id = call.get("id").and_then(|v| v.as_str()).map(String::from);
                    let function = call.get("function");
                    let name = function
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let arguments = function
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    return Ok(Some(MessageStreamEvent::ToolCallDelta(
                        wf_types::llm::MessageStreamToolCallDelta {
                            index,
                            id,
                            name,
                            arguments,
                            is_snapshot: false,
                        },
                    )));
                }
            }
        }
        if let Some(finish_reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
            let is_done = matches!(
                finish_reason,
                "stop" | "tool_calls" | "length" | "content_filter"
            );
            if is_done {
                return Ok(Some(MessageStreamEvent::End(
                    wf_types::llm::MessageStreamEnd {},
                )));
            }
        }
    }

    Ok(None)
}

pub fn convert_openai_tools(tools: &[wf_types::tool::Tool]) -> LlmResult<Vec<serde_json::Value>> {
    let tool_defs: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect();
    Ok(tool_defs)
}

/// Apply authentication, custom headers and query parameters to the request.
///
/// Auth type resolution: `profile.auth_type` wins when set ("native" /
/// "bearer" / "x-api-key"), otherwise `default_auth` is used. "native" maps
/// to the provider-specific header (x-api-key for Anthropic, x-goog-api-key
/// for Gemini, Bearer for OpenAI).
pub fn apply_auth_and_headers(
    req_builder: reqwest::RequestBuilder,
    profile: &LlmProfile,
    default_auth: &str,
) -> reqwest::RequestBuilder {
    let mut builder = req_builder;

    if let Some(api_key) = &profile.api_key {
        let auth_type = profile.auth_type.as_deref().unwrap_or(default_auth);
        match auth_type {
            "native" => match profile.provider {
                wf_types::llm::LlmProvider::Anthropic => {
                    builder = builder.header("x-api-key", api_key);
                }
                wf_types::llm::LlmProvider::GeminiNative
                | wf_types::llm::LlmProvider::GeminiOpenai => {
                    builder = builder.header("x-goog-api-key", api_key);
                }
                _ => {
                    builder = builder.header("Authorization", format!("Bearer {}", api_key));
                }
            },
            "x-api-key" => {
                builder = builder.header("x-api-key", api_key);
            }
            _ => {
                builder = builder.header("Authorization", format!("Bearer {}", api_key));
            }
        }
    }

    if let Some(headers) = &profile.headers {
        for (key, value) in headers.iter() {
            builder = builder.header(key, value.as_str().unwrap_or(""));
        }
    }
    if let Some(custom_headers) = &profile.custom_headers {
        for (key, value) in custom_headers.iter() {
            builder = builder.header(key, value.as_str().unwrap_or(""));
        }
    }
    if let Some(params) = &profile.query_params {
        let pairs: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str().unwrap_or("")))
            .collect();
        builder = builder.query(&pairs);
    }

    builder
}

/// Deep-merge `profile.custom_body` into the request body (custom body wins),
/// governed by `custom_body_enabled`.
pub fn apply_custom_body(body: &mut serde_json::Value, profile: &LlmProfile) {
    if profile.custom_body_enabled.unwrap_or(true) {
        if let Some(custom) = &profile.custom_body {
            *body = crate::formatter_helpers::deep_merge(body, custom);
        }
    }
}

pub fn add_auth_and_headers(
    req_builder: reqwest::RequestBuilder,
    profile: &LlmProfile,
    auth_type: &str,
) -> reqwest::RequestBuilder {
    apply_auth_and_headers(req_builder, profile, auth_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::MessageStreamEvent;

    #[test]
    fn stream_usage_chunk_is_extracted() {
        let chunk = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
        let event = parse_openai_stream_chunk(chunk).expect("chunk must parse");
        match event {
            Some(MessageStreamEvent::Usage(usage)) => {
                assert_eq!(usage.usage.prompt_tokens, 10);
                assert_eq!(usage.usage.completion_tokens, 5);
                assert_eq!(usage.usage.total_tokens, 15);
            }
            other => panic!("expected Usage event, got {:?}", other),
        }
    }

    #[test]
    fn empty_choices_without_usage_is_ignored() {
        let chunk = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","choices":[]}"#;
        assert!(parse_openai_stream_chunk(chunk)
            .expect("chunk must parse")
            .is_none());
    }

    #[test]
    fn stream_tool_call_delta_is_extracted() {
        let chunk = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_weather","arguments":"{\"city\":\"Bei"}}]}}]}"#;
        match parse_openai_stream_chunk(chunk).expect("chunk must parse") {
            Some(MessageStreamEvent::ToolCallDelta(delta)) => {
                assert_eq!(delta.index, 0);
                assert_eq!(delta.id.as_deref(), Some("call_1"));
                assert_eq!(delta.name.as_deref(), Some("get_weather"));
                assert_eq!(delta.arguments.as_deref(), Some(r#"{"city":"Bei"#));
                assert!(!delta.is_snapshot);
            }
            other => panic!("expected ToolCallDelta, got {:?}", other),
        }
    }
}

#[cfg(test)]
mod enhancement_tests {
    use super::*;
    use wf_types::llm::LlmProvider;

    fn profile_with(auth_type: Option<&str>) -> LlmProfile {
        LlmProfile {
            id: "p1".to_string(),
            name: "test".to_string(),
            provider: LlmProvider::Anthropic,
            model: "claude-3-5-sonnet".to_string(),
            api_key: Some("sk-test".to_string()),
            base_url: None,
            parameters: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_format: None,
            auth_type: auth_type.map(String::from),
            custom_headers: Some(
                [("x-custom".to_string(), serde_json::json!("v1"))]
                    .into_iter()
                    .collect(),
            ),
            custom_body: None,
            custom_body_enabled: None,
            query_params: Some(
                [("api-version".to_string(), serde_json::json!("2024-01"))]
                    .into_iter()
                    .collect(),
            ),
            stream_options: None,
            context_window_size: None,
        }
    }

    #[test]
    fn native_auth_uses_provider_header() {
        let req =
            reqwest::Client::new().request(reqwest::Method::POST, "https://example.com/messages");
        let req = apply_auth_and_headers(req, &profile_with(Some("native")), "bearer")
            .build()
            .unwrap();
        assert_eq!(
            req.headers().get("x-api-key").unwrap(),
            "sk-test",
            "anthropic native auth must use x-api-key"
        );
        assert!(req.headers().get("Authorization").is_none());
    }

    #[test]
    fn bearer_auth_overrides_default() {
        let req =
            reqwest::Client::new().request(reqwest::Method::POST, "https://example.com/messages");
        let req = apply_auth_and_headers(req, &profile_with(Some("bearer")), "x-api-key")
            .build()
            .unwrap();
        assert!(req.headers().get("x-api-key").is_none());
        assert_eq!(
            req.headers().get("Authorization").unwrap(),
            "Bearer sk-test"
        );
    }

    #[test]
    fn custom_headers_and_query_params_are_applied() {
        let req =
            reqwest::Client::new().request(reqwest::Method::POST, "https://example.com/messages");
        let req = apply_auth_and_headers(req, &profile_with(None), "bearer")
            .build()
            .unwrap();
        assert_eq!(req.headers().get("x-custom").unwrap(), "v1");
        assert_eq!(req.url().query().unwrap(), "api-version=2024-01");
    }

    #[test]
    fn params_pass_through_untouched() {
        let mut profile = profile_with(None);
        profile.parameters = Some(serde_json::json!({
            "response_format": {"type": "json_object"},
            "frequency_penalty": 0.5,
        }));
        let mut body = serde_json::json!({"model": "m", "messages": []});
        merge_and_apply_params(
            &mut body,
            &profile,
            &Some(serde_json::json!({
                "temperature": 0.2,
                "reasoning_effort": "high",
                "stream": true,
            })),
        );

        assert_eq!(
            body["response_format"]["type"],
            serde_json::json!("json_object")
        );
        assert_eq!(body["frequency_penalty"], serde_json::json!(0.5));
        assert_eq!(body["temperature"], serde_json::json!(0.2));
        assert_eq!(body["reasoning_effort"], serde_json::json!("high"));
        assert!(
            body.get("stream").is_none(),
            "stream must be controlled by the caller"
        );
    }
}
