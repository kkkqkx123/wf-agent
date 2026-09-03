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
            let history = shared::convert_history_for_text_mode(&filtered, request);
            let mut converted = shared::convert_openai_messages(&history);
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

    // The OpenAI-compatible Gemini endpoint exposes no counting API (the
    // native `:countTokens` endpoint belongs to a different protocol and
    // auth scheme): keep the default `None` so the caller falls back to
    // local estimation.
    fn build_count_tokens_request(
        &self,
        _request: &LlmRequest,
        _profile: &LlmProfile,
    ) -> LlmResult<Option<reqwest::Request>> {
        Ok(None)
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

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::{
        LlmProvider, MessageStreamEvent, ToolCallFormat, ToolCallFormatConfig, ToolCallMarkers,
    };
    use wf_types::message::{Message, MessageContentValue, MessageRole};
    use wf_types::tool::Tool;

    fn profile(base_url: Option<&str>) -> LlmProfile {
        LlmProfile {
            id: "p1".to_string(),
            name: "test".to_string(),
            provider: LlmProvider::GeminiOpenai,
            model: "gemini-2.0-flash".to_string(),
            api_key: Some("gsk-test".to_string()),
            base_url: base_url.map(String::from),
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

    fn request(text: &str, format: Option<ToolCallFormat>) -> LlmRequest {
        LlmRequest {
            profile_id: "p1".to_string(),
            messages: vec![Message {
                id: wf_types::Id::new(),
                role: MessageRole::User,
                content: MessageContentValue::Text(text.to_string()),
                timestamp: 0,
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
            parameters: None,
            tools: None,
            tool_call_format: format,
            locked_tool_call_format: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        }
    }

    #[test]
    fn build_request_uses_default_base_url() {
        let formatter = GeminiOpenaiFormatter::new();
        let req = formatter
            .build_request(&request("hi", None), &profile(None))
            .unwrap();
        assert_eq!(
            req.url().as_str(),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
        assert_eq!(req.method(), reqwest::Method::POST);
        assert_eq!(
            req.headers().get("Authorization").unwrap(),
            "Bearer gsk-test",
            "default auth is bearer for the OpenAI-compatible endpoint"
        );
    }

    #[test]
    fn build_request_profile_base_url_wins() {
        let formatter = GeminiOpenaiFormatter::new();
        let req = formatter
            .build_request(
                &request("hi", None),
                &profile(Some("https://proxy.example.com")),
            )
            .unwrap();
        assert_eq!(
            req.url().as_str(),
            "https://proxy.example.com/chat/completions"
        );
    }

    #[test]
    fn build_request_carries_messages_and_tools() {
        let formatter = GeminiOpenaiFormatter::new();
        let mut req = request("hi", Some(ToolCallFormat::Native));
        req.tools = Some(vec![Tool {
            id: wf_types::Id::new(),
            name: "search".to_string(),
            description: "Search the web".to_string(),
            tool_type: wf_types::tool::ToolType::BuiltIn,
            parameters: None,
            metadata: None,
            config: None,
            enabled: None,
            strict: None,
            default_timeout_ms: None,
        }]);
        req.stream = Some(true);
        let http_req = formatter.build_request(&req, &profile(None)).unwrap();
        let body: serde_json::Value = http_req
            .body()
            .unwrap()
            .as_bytes()
            .map(|b| serde_json::from_slice(b).unwrap())
            .unwrap();
        assert_eq!(body["model"], serde_json::json!("gemini-2.0-flash"));
        assert_eq!(body["messages"][0]["content"], serde_json::json!("hi"));
        assert_eq!(body["stream"], serde_json::json!(true));
        assert_eq!(
            body["tools"][0]["function"]["name"],
            serde_json::json!("search")
        );
    }

    #[test]
    fn text_mode_injects_system_content_and_skips_native_tools() {
        let formatter = GeminiOpenaiFormatter::new();
        let mut req = request("hi", Some(ToolCallFormat::JsonWrapped));
        req.messages.insert(
            0,
            Message {
                id: wf_types::Id::new(),
                role: MessageRole::System,
                content: MessageContentValue::Text("you are a bot".to_string()),
                timestamp: 0,
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            },
        );
        req.tools = Some(vec![Tool {
            id: wf_types::Id::new(),
            name: "search".to_string(),
            description: "Search the web".to_string(),
            tool_type: wf_types::tool::ToolType::BuiltIn,
            parameters: None,
            metadata: None,
            config: None,
            enabled: None,
            strict: None,
            default_timeout_ms: None,
        }]);
        let http_req = formatter.build_request(&req, &profile(None)).unwrap();
        let body: serde_json::Value = http_req
            .body()
            .unwrap()
            .as_bytes()
            .map(|b| serde_json::from_slice(b).unwrap())
            .unwrap();
        let system = body["messages"][0].clone();
        assert_eq!(system["role"], serde_json::json!("system"));
        let content = system["content"].as_str().unwrap();
        assert!(content.contains("you are a bot"));
        assert!(content.contains("Available Tools"));
        assert!(
            body.get("tools").is_none(),
            "text mode must not attach native tool schemas"
        );
    }

    #[test]
    fn parse_response_extracts_text_tool_calls_in_text_mode() {
        let formatter = GeminiOpenaiFormatter::new();
        let body = r#"{
            "id": "chatcmpl-1",
            "model": "gemini-2.0-flash",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "<<<TOOL_CALL>>>\n{\"tool\": \"search\", \"parameters\": {\"q\": \"rust\"}}\n<<<END_TOOL_CALL>>>"
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;
        let req = request("hi", Some(ToolCallFormat::JsonWrapped));
        let result = formatter.parse_response(body, &req).unwrap();
        assert!(result.content.as_deref().unwrap().contains("TOOL_CALL"));
        let calls = result.tool_calls.as_ref().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "search");
        assert_eq!(result.usage.as_ref().unwrap().total_tokens, 15);
        assert_eq!(result.message.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn parse_response_keeps_native_tool_calls_in_native_mode() {
        let formatter = GeminiOpenaiFormatter::new();
        let body = r#"{
            "id": "chatcmpl-1",
            "model": "gemini-2.0-flash",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "plain answer",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "search", "arguments": "{\"q\":\"x\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }"#;
        let req = request("hi", Some(ToolCallFormat::Native));
        let result = formatter.parse_response(body, &req).unwrap();
        assert_eq!(result.content.as_deref(), Some("plain answer"));
        assert_eq!(result.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn stream_chunk_passthrough_matches_openai() {
        let formatter = GeminiOpenaiFormatter::new();
        let chunk = r#"{"id":"chatcmpl-1","choices":[{"index":0,"delta":{"content":"hi there"},"finish_reason":null}]}"#;
        match formatter.parse_stream_chunk(chunk).unwrap() {
            Some(MessageStreamEvent::Text(t)) => assert_eq!(t.text, "hi there"),
            other => panic!("expected Text event, got {other:?}"),
        }
        let done = formatter
            .parse_stream_chunk("[DONE]")
            .expect("end marker parses");
        assert!(matches!(done, Some(MessageStreamEvent::End(_))));
        assert!(formatter.parse_stream_chunk("not json").is_err());
    }

    #[test]
    fn locked_format_with_custom_markers_drives_parsing() {
        let formatter = GeminiOpenaiFormatter::new();
        let req = LlmRequest {
            profile_id: "p1".to_string(),
            messages: Vec::new(),
            parameters: None,
            tools: None,
            // The gateway always mirrors the locked format into
            // `tool_call_format`; the custom markers come from the lock.
            tool_call_format: Some(ToolCallFormat::JsonWrapped),
            locked_tool_call_format: Some(ToolCallFormatConfig {
                format: ToolCallFormat::JsonWrapped,
                markers: Some(ToolCallMarkers {
                    start: Some("<<<TOOL>>>".to_string()),
                    end: Some("<<<END>>>".to_string()),
                }),
                xml_tags: None,
                include_description: None,
                description_style: None,
                include_examples: None,
                include_rules: None,
                additional_config: None,
            }),
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        };
        let body = r#"{
            "id": "1",
            "model": "gemini-2.0-flash",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "<<<TOOL>>> {\"tool\": \"lookup\"} <<<END>>>"
                },
                "finish_reason": "tool_calls"
            }]
        }"#;
        let result = formatter.parse_response(body, &req).unwrap();
        let calls = result.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "lookup");
    }
}
