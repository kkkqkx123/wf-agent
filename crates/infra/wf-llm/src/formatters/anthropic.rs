use super::LlmFormatter;
use crate::error::LlmResult;
use reqwest::Method;
use wf_types::llm::{LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent};
use wf_types::tool::Tool;

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

    /// Build the request body (separated from the HTTP layer for testability).
    fn build_body(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<serde_json::Value> {
        let use_text_mode = super::shared::is_text_mode(request);

        // `convert_messages` skips system messages; the system content is sent
        // in the dedicated `system` field. Text mode injects the original
        // system + tool usage instructions + declarations; native mode keeps
        // the original system message.
        let (system_content, _) = crate::tool_format::extract_system_message(&request.messages);

        let history = if use_text_mode {
            super::shared::convert_history_for_text_mode(&request.messages, request)
        } else {
            request.messages.clone()
        };
        let messages = self.convert_messages(&history);

        let mut body = serde_json::json!({
            "model": profile.model,
            "messages": messages,
        });

        if use_text_mode {
            let system = super::shared::text_mode_system_content(request);
            if !system.is_empty() {
                body["system"] = serde_json::json!(system);
            }
        } else if let Some(system) = system_content {
            if !system.is_empty() {
                body["system"] = serde_json::json!(system);
            }
        }

        let generation = super::shared::resolve_generation(request, profile)?;
        crate::generation::apply_anthropic(&mut body, &generation)?;
        super::shared::merge_and_apply_params(&mut body, profile, &request.parameters);

        if !use_text_mode {
            if let Some(tools) = &request.tools {
                let tool_defs: Vec<serde_json::Value> = tools
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "description": t.description,
                            "input_schema": t.parameters,
                        })
                    })
                    .collect();
                body["tools"] = serde_json::json!(tool_defs);
            }
        }

        if request.stream == Some(true) {
            body["stream"] = serde_json::json!(true);
        }

        super::shared::apply_custom_body(&mut body, profile);

        Ok(body)
    }

    /// Build the count-tokens request body (system included so the count
    /// matches what `build_request` sends).
    fn build_count_tokens_body(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<serde_json::Value> {
        let messages = self.convert_messages(&request.messages);

        let mut body = serde_json::json!({
            "model": profile.model,
            "messages": messages,
        });

        let (system_content, _) = crate::tool_format::extract_system_message(&request.messages);
        if let Some(system) = system_content {
            if !system.is_empty() {
                body["system"] = serde_json::json!(system);
            }
        }

        if let Some(tools) = &request.tools {
            let tool_defs: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters,
                    })
                })
                .collect();
            body["tools"] = serde_json::json!(tool_defs);
        }

        Ok(body)
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
                    if text.is_empty() {
                        continue;
                    }
                    serde_json::json!([{"type": "text", "text": text}])
                }
                wf_types::message::MessageContentValue::Rich(blocks) => {
                    let converted: Vec<serde_json::Value> = blocks
                        .iter()
                        .map(|block| match block {
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
                                let (media_type, data) =
                                    if let Some(rest) = url.strip_prefix("data:") {
                                        if let Some(semicolon) = rest.find(';') {
                                            let mime = &rest[..semicolon];
                                            let b64 = rest[semicolon + 1..]
                                                .strip_prefix("base64,")
                                                .unwrap_or("")
                                                .to_string();
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
                        })
                        .collect();
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
                        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
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
    fn build_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<reqwest::Request> {
        let url = format!(
            "{}/messages",
            profile.base_url.as_deref().unwrap_or(&self.base_url)
        );

        let body = self.build_body(request, profile)?;

        let mut req_builder = reqwest::Client::new()
            .request(Method::POST, &url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&body);

        req_builder = super::shared::apply_auth_and_headers(req_builder, profile, "x-api-key");

        req_builder
            .build()
            .map_err(crate::error::LlmError::HttpError)
    }

    fn build_count_tokens_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<Option<reqwest::Request>> {
        let url = format!(
            "{}/messages/count_tokens",
            profile.base_url.as_deref().unwrap_or(&self.base_url)
        );

        let body = self.build_count_tokens_body(request, profile)?;

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

        req_builder
            .build()
            .map(Some)
            .map_err(crate::error::LlmError::HttpError)
    }

    fn parse_response(&self, body: &str, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        let mut result = self.parse_anthropic_response(body)?;
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
        let event_type = json.get("type").and_then(|v| v.as_str());

        match event_type {
            Some("message_start") => Ok(None),
            Some("content_block_start") => {
                let block = json.get("content_block");
                if let Some(b) = block {
                    if b.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                        let index =
                            json.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let id = b.get("id").and_then(|v| v.as_str()).map(String::from);
                        let name = b.get("name").and_then(|v| v.as_str()).map(String::from);
                        return Ok(Some(MessageStreamEvent::ToolCallDelta(
                            wf_types::llm::MessageStreamToolCallDelta {
                                index,
                                id,
                                name,
                                arguments: None,
                                is_snapshot: false,
                            },
                        )));
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
                                    wf_types::llm::MessageStreamText {
                                        text: text.to_string(),
                                        snapshot: text.to_string(),
                                    },
                                )));
                            }
                        }
                        Some("thinking_delta") => {
                            if let Some(reasoning) = d.get("thinking").and_then(|v| v.as_str()) {
                                return Ok(Some(MessageStreamEvent::ReasoningText(
                                    wf_types::llm::MessageStreamReasoning {
                                        reasoning: reasoning.to_string(),
                                        snapshot: reasoning.to_string(),
                                    },
                                )));
                            }
                        }
                        Some("input_json_delta") => {
                            if let Some(partial) = d.get("partial_json").and_then(|v| v.as_str()) {
                                let index = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0)
                                    as usize;
                                return Ok(Some(MessageStreamEvent::ToolCallDelta(
                                    wf_types::llm::MessageStreamToolCallDelta {
                                        index,
                                        id: None,
                                        name: None,
                                        arguments: Some(partial.to_string()),
                                        is_snapshot: false,
                                    },
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Ok(None)
            }
            Some("message_delta") => {
                // Usage is reported as a cumulative counter on every delta;
                // surface it as a stream event so the gateway records it.
                if let Some(usage) = json.get("usage") {
                    return Ok(Some(MessageStreamEvent::Usage(
                        wf_types::llm::MessageStreamUsage {
                            usage: wf_types::llm::TokenUsageStats {
                                prompt_tokens: usage
                                    .get("input_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0)
                                    as u32,
                                completion_tokens: usage
                                    .get("output_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0)
                                    as u32,
                                total_tokens: 0,
                                reasoning_tokens: usage
                                    .get("thinking_tokens")
                                    .and_then(|v| v.as_u64())
                                    .map(|r| r as u32),
                                cache_read_tokens: usage
                                    .get("cache_read_input_tokens")
                                    .and_then(|v| v.as_u64())
                                    .map(|r| r as u32),
                                cache_write_tokens: usage
                                    .get("cache_creation_input_tokens")
                                    .and_then(|v| v.as_u64())
                                    .map(|r| r as u32),
                                prompt_tokens_cost: None,
                                completion_tokens_cost: None,
                                total_cost: None,
                            },
                        },
                    )));
                }
                let delta = json.get("delta");
                if let Some(d) = delta {
                    if let Some(stop_reason) = d.get("stop_reason").and_then(|v| v.as_str()) {
                        let is_done = matches!(
                            stop_reason,
                            "end_turn" | "max_tokens" | "stop_sequence" | "tool_use"
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
            Some("message_stop") => Ok(Some(MessageStreamEvent::End(
                wf_types::llm::MessageStreamEnd {},
            ))),
            _ => Ok(None),
        }
    }

    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>> {
        let tool_defs: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect();
        Ok(tool_defs)
    }

    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall> {
        result.tool_calls.clone().unwrap_or_default()
    }
}

impl AnthropicFormatter {
    fn parse_anthropic_response(&self, body: &str) -> LlmResult<LlmResponseType> {
        let json: serde_json::Value = serde_json::from_str(body)?;

        let id = json.get("id").and_then(|v| v.as_str()).map(String::from);
        let model = json
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let stop_reason = json
            .get("stop_reason")
            .and_then(|v| v.as_str())
            .map(String::from);

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
                        let tc_id = block
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let input = block
                            .get("input")
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
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
                cache_read_tokens: u
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64())
                    .map(|r| r as u32),
                cache_write_tokens: u
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64())
                    .map(|r| r as u32),
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
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls.clone())
            },
            thinking: None,
            metadata: None,
        };

        let reasoning_token_count = json
            .get("usage")
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
        let metadata = if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        };

        Ok(LlmResponseType {
            id,
            model,
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
            finish_reason: stop_reason,
            duration: 0,
            reasoning_content: None,
            reasoning_tokens: reasoning_token_count.map(|r| r as u32),
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
    fn content_block_start_tool_use_emits_name_and_id() {
        let chunk = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"get_weather","input":{}}}"#;
        match AnthropicFormatter::new()
            .parse_stream_chunk(chunk)
            .expect("chunk must parse")
        {
            Some(MessageStreamEvent::ToolCallDelta(delta)) => {
                assert_eq!(delta.index, 1);
                assert_eq!(delta.id.as_deref(), Some("toolu_1"));
                assert_eq!(delta.name.as_deref(), Some("get_weather"));
                assert_eq!(delta.arguments, None);
            }
            other => panic!("expected ToolCallDelta, got {:?}", other),
        }
    }

    #[test]
    fn input_json_delta_emits_arguments_fragment() {
        let chunk = r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"city\":\"Beijing\"}"}}"#;
        match AnthropicFormatter::new()
            .parse_stream_chunk(chunk)
            .expect("chunk must parse")
        {
            Some(MessageStreamEvent::ToolCallDelta(delta)) => {
                assert_eq!(delta.index, 1);
                assert_eq!(delta.arguments.as_deref(), Some(r#"{"city":"Beijing"}"#));
                assert!(delta.name.is_none());
            }
            other => panic!("expected ToolCallDelta, got {:?}", other),
        }
    }

    fn profile() -> LlmProfile {
        LlmProfile {
            id: "p1".to_string(),
            name: "test".to_string(),
            provider: wf_types::llm::LlmProvider::Anthropic,
            model: "claude-3-5-sonnet".to_string(),
            api_key: Some("sk-test".to_string()),
            base_url: None,
            parameters: None,
            generation: None,
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
            generation: None,
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
    fn native_mode_sends_system_field() {
        let formatter = AnthropicFormatter::new();
        let body = formatter
            .build_body(
                &request(
                    vec![
                        msg(wf_types::message::MessageRole::System, "You are a helper"),
                        msg(wf_types::message::MessageRole::User, "Hello"),
                    ],
                    None,
                ),
                &profile(),
            )
            .expect("build must succeed");

        assert_eq!(body["system"], serde_json::json!("You are a helper"));
        let roles: Vec<&str> = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["role"].as_str())
            .collect();
        assert_eq!(roles, vec!["user"], "system must not appear in messages");
    }

    #[test]
    fn text_mode_system_field_includes_tool_instructions() {
        let formatter = AnthropicFormatter::new();
        let mut req = request(
            vec![
                msg(wf_types::message::MessageRole::System, "You are a helper"),
                msg(wf_types::message::MessageRole::User, "Hello"),
            ],
            None,
        );
        req.tool_call_format = Some(wf_types::llm::ToolCallFormat::Xml);
        let body = formatter.build_body(&req, &profile()).expect("must build");

        let system = body["system"].as_str().unwrap();
        assert!(system.contains("You are a helper"));
        assert!(system.contains("<tool_use>"), "must carry instructions");
    }

    #[test]
    fn params_pass_through_except_stream_and_stop() {
        let formatter = AnthropicFormatter::new();
        let req = request(
            vec![msg(wf_types::message::MessageRole::User, "Hello")],
            Some(serde_json::json!({
                "temperature": 0.2,
                "thinking": {"type": "enabled", "budget_tokens": 1024},
                "tool_choice": {"type": "auto"},
                "stop": ["END"],
                "stream": true,
            })),
        );
        let body = formatter.build_body(&req, &profile()).expect("must build");

        assert_eq!(body["temperature"], serde_json::json!(0.2));
        assert_eq!(body["thinking"]["budget_tokens"], serde_json::json!(1024));
        assert_eq!(body["tool_choice"]["type"], serde_json::json!("auto"));
        assert_eq!(body["stop_sequences"], serde_json::json!(["END"]));
        assert!(body.get("stop").is_none(), "stop must be mapped");
        assert!(body.get("stream").is_none(), "stream controlled by caller");
    }

    #[test]
    fn count_tokens_includes_system() {
        let formatter = AnthropicFormatter::new();
        let req = request(
            vec![msg(
                wf_types::message::MessageRole::System,
                "You are a helper",
            )],
            None,
        );
        let body = formatter
            .build_count_tokens_body(&req, &profile())
            .expect("must build");
        assert_eq!(body["system"], serde_json::json!("You are a helper"));
    }
}
