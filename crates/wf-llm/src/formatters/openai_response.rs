use super::LlmFormatter;
use crate::error::LlmResult;
use reqwest::Method;
use std::collections::HashMap;
use std::sync::Mutex;
use wf_types::llm::{LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent};
use wf_types::tool::Tool;

pub struct OpenaiResponseFormatter {
    base_url: String,
    /// Maps streaming `item_id` to the accumulated tool call index so
    /// `function_call_arguments.delta` fragments land on the same call as the
    /// final `function_call.done` snapshot.
    call_indices: Mutex<HashMap<String, usize>>,
    next_index: Mutex<usize>,
}

impl Default for OpenaiResponseFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenaiResponseFormatter {
    pub fn new() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            call_indices: Mutex::new(HashMap::new()),
            next_index: Mutex::new(0),
        }
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self {
            base_url,
            call_indices: Mutex::new(HashMap::new()),
            next_index: Mutex::new(0),
        }
    }

    fn call_index_for(&self, item_id: &str) -> usize {
        let mut indices = wf_common::lock::lock_ok(self.call_indices.lock());
        if let Some(idx) = indices.get(item_id) {
            return *idx;
        }
        let mut next = wf_common::lock::lock_ok(self.next_index.lock());
        let idx = *next;
        *next += 1;
        indices.insert(item_id.to_string(), idx);
        idx
    }

    fn convert_messages(&self, messages: &[wf_types::message::Message]) -> Vec<serde_json::Value> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    wf_types::message::MessageRole::System => "system",
                    wf_types::message::MessageRole::User => "user",
                    wf_types::message::MessageRole::Assistant => "assistant",
                    wf_types::message::MessageRole::Tool => "tool",
                };

                let mut entry = serde_json::json!({
                    "role": role,
                    "content": msg.content,
                });

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

    fn parse_tool_calls_from_output(
        &self,
        output: &[serde_json::Value],
    ) -> Vec<wf_types::message::LlmToolCall> {
        output
            .iter()
            .filter_map(|item| {
                let item_type = item.get("type").and_then(|v| v.as_str())?;
                if item_type != "function_call" {
                    return None;
                }
                let id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = item
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}")
                    .to_string();
                Some(wf_types::message::LlmToolCall {
                    id,
                    r#type: "function".to_string(),
                    function: wf_types::message::LlmFunctionCall {
                        name: String::new(),
                        arguments,
                    },
                })
            })
            .collect()
    }
}

impl LlmFormatter for OpenaiResponseFormatter {
    fn build_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<reqwest::Request> {
        let url = format!(
            "{}/responses",
            profile.base_url.as_deref().unwrap_or(&self.base_url)
        );

        let use_text_mode = super::shared::is_text_mode(request);

        let mut body = serde_json::json!({
            "model": profile.model,
            "input": self.convert_messages(&request.messages),
        });

        if use_text_mode {
            let history = super::shared::convert_history_for_text_mode(&request.messages, request);
            body["input"] = serde_json::json!(self.convert_messages(&history));
            let system = super::shared::text_mode_system_content(request);
            if !system.is_empty() {
                let mut input = body["input"].take();
                if let Some(arr) = input.as_array_mut() {
                    arr.insert(0, serde_json::json!({"role": "system", "content": system}));
                }
                body["input"] = input;
            }
        }

        if request.stream == Some(true) {
            body["stream"] = serde_json::json!(true);
        }

        // Pass through all merged parameters untouched (matching the
        // `Object.assign(body, otherParams)` behavior), except `stream` which
        // is controlled by the caller above.
        let merged_params =
            crate::formatter_helpers::merge_parameters(profile, &request.parameters);
        for (key, value) in merged_params {
            if key != "stream" {
                body[key] = value;
            }
        }

        if !use_text_mode {
            if let Some(tools) = &request.tools {
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
                body["tools"] = serde_json::json!(tool_defs);
            }
        }

        super::shared::apply_custom_body(&mut body, profile);

        let mut req_builder = reqwest::Client::new()
            .request(Method::POST, &url)
            .header("Content-Type", "application/json")
            .json(&body);

        req_builder = super::shared::apply_auth_and_headers(req_builder, profile, "bearer");

        req_builder
            .build()
            .map_err(crate::error::LlmError::HttpError)
    }

    fn parse_response(&self, body: &str, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        let mut result = self.parse_response_inner(body)?;
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
            Some("response.output_text.delta") => {
                if let Some(delta) = json.get("delta").and_then(|v| v.as_str()) {
                    return Ok(Some(MessageStreamEvent::Text(
                        wf_types::llm::MessageStreamText {
                            snapshot: String::new(),
                            text: delta.to_string(),
                        },
                    )));
                }
            }
            Some("response.function_call_arguments.delta") => {
                // Incremental arguments fragment; grouped by item_id onto a
                // stable index (mirrors the toolCallsDelta accumulation).
                let item_id = json
                    .get("item_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if let Some(delta) = json.get("delta").and_then(|v| v.as_str()) {
                    let index = self.call_index_for(&item_id);
                    return Ok(Some(MessageStreamEvent::ToolCallDelta(
                        wf_types::llm::MessageStreamToolCallDelta {
                            index,
                            id: None,
                            name: None,
                            arguments: Some(delta.to_string()),
                            is_snapshot: false,
                        },
                    )));
                }
            }
            Some("response.function_call.done") => {
                // Complete snapshot: name + full arguments, replacing the
                // accumulated fragments on the same index.
                let item = json.get("item");
                let item_id = item
                    .and_then(|i| i.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let index = self.call_index_for(&item_id);
                let name = item
                    .and_then(|i| i.get("name"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let arguments = item
                    .and_then(|i| i.get("arguments"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                return Ok(Some(MessageStreamEvent::ToolCallDelta(
                    wf_types::llm::MessageStreamToolCallDelta {
                        index,
                        id: Some(item_id),
                        name,
                        arguments,
                        is_snapshot: true,
                    },
                )));
            }
            Some("response.completed") | Some("response.incomplete") => {
                // The completed event carries the final usage summary.
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
                                total_tokens: usage
                                    .get("total_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0)
                                    as u32,
                                reasoning_tokens: usage
                                    .get("output_tokens_details")
                                    .and_then(|d| d.get("reasoning_tokens"))
                                    .and_then(|v| v.as_u64())
                                    .map(|r| r as u32),
                                cache_read_tokens: usage
                                    .get("input_tokens_details")
                                    .and_then(|d| d.get("cached_tokens"))
                                    .and_then(|v| v.as_u64())
                                    .map(|r| r as u32),
                                cache_write_tokens: None,
                                prompt_tokens_cost: None,
                                completion_tokens_cost: None,
                                total_cost: None,
                            },
                        },
                    )));
                }
                return Ok(Some(MessageStreamEvent::End(
                    wf_types::llm::MessageStreamEnd {},
                )));
            }
            _ => {}
        }

        Ok(None)
    }

    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>> {
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

    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall> {
        result.tool_calls.clone().unwrap_or_default()
    }
}

impl OpenaiResponseFormatter {
    fn parse_response_inner(&self, body: &str) -> LlmResult<LlmResponseType> {
        let json: serde_json::Value = serde_json::from_str(body)?;

        let id = json.get("id").and_then(|v| v.as_str()).map(String::from);
        let model = json
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let _status = json
            .get("status")
            .and_then(|v| v.as_str())
            .map(String::from);

        let output = json.get("output").and_then(|v| v.as_array());
        let last_output = output.and_then(|o| o.last());

        let content = last_output
            .and_then(|o| o.get("content"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|block| block.get("text"))
            .and_then(|t| t.as_str())
            .map(String::from);

        let mut tool_calls: Option<Vec<wf_types::message::LlmToolCall>> = None;
        if let Some(output_arr) = output {
            let calls = self.parse_tool_calls_from_output(output_arr);
            if !calls.is_empty() {
                tool_calls = Some(calls);
            }
        }

        let usage = json.get("usage").map(|u| {
            let input_tokens = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let output_tokens = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            wf_types::llm::TokenUsageStats {
                prompt_tokens: input_tokens,
                completion_tokens: output_tokens,
                total_tokens: input_tokens + output_tokens,
                reasoning_tokens: u
                    .get("output_tokens_details")
                    .and_then(|d| d.get("reasoning_tokens"))
                    .and_then(|v| v.as_u64())
                    .map(|r| r as u32),
                cache_read_tokens: u
                    .get("input_tokens_details")
                    .and_then(|d| d.get("cached_tokens"))
                    .and_then(|v| v.as_u64())
                    .map(|r| r as u32),
                cache_write_tokens: None,
                prompt_tokens_cost: None,
                completion_tokens_cost: None,
                total_cost: None,
            }
        });

        let message = wf_types::message::Message {
            id: wf_types::Id::new(),
            role: wf_types::message::MessageRole::Assistant,
            content: wf_types::message::MessageContentValue::Text(
                content.clone().unwrap_or_default(),
            ),
            timestamp: wf_common::time::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: tool_calls.clone(),
            thinking: None,
            metadata: None,
        };

        let finish_reason = json
            .get("status")
            .and_then(|s| s.as_str())
            .map(String::from);

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
            metadata: None,
            stream_stats: None,
            warnings: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_call_arguments_delta_and_done_share_index() {
        let formatter = OpenaiResponseFormatter::new();

        let delta = r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"{\"city\":"}"#;
        match formatter
            .parse_stream_chunk(delta)
            .expect("chunk must parse")
        {
            Some(MessageStreamEvent::ToolCallDelta(d)) => {
                assert_eq!(d.index, 0);
                assert_eq!(d.arguments.as_deref(), Some(r#"{"city":"#));
                assert!(!d.is_snapshot);
            }
            other => panic!("expected ToolCallDelta, got {:?}", other),
        }

        let done = r#"{"type":"response.function_call.done","item":{"type":"function_call","id":"fc_1","name":"get_weather","arguments":"{\"city\":\"Beijing\"}"}}"#;
        match formatter
            .parse_stream_chunk(done)
            .expect("chunk must parse")
        {
            Some(MessageStreamEvent::ToolCallDelta(d)) => {
                assert_eq!(d.index, 0, "fragments must land on the same call");
                assert_eq!(d.id.as_deref(), Some("fc_1"));
                assert_eq!(d.name.as_deref(), Some("get_weather"));
                assert_eq!(d.arguments.as_deref(), Some(r#"{"city":"Beijing"}"#));
                assert!(d.is_snapshot);
            }
            other => panic!("expected ToolCallDelta, got {:?}", other),
        }
    }

    #[test]
    fn stream_chunks_cover_text_completion_and_usage() {
        let formatter = OpenaiResponseFormatter::new();

        match formatter
            .parse_stream_chunk(r#"{"type":"response.output_text.delta","delta":"Hello "}"#)
            .expect("text chunk parses")
        {
            Some(MessageStreamEvent::Text(t)) => assert_eq!(t.text, "Hello "),
            other => panic!("expected Text, got {other:?}"),
        }

        match formatter
            .parse_stream_chunk(
                r#"{"type":"response.completed","response":{"status":"completed"},"usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15,"output_tokens_details":{"reasoning_tokens":2},"input_tokens_details":{"cached_tokens":3}}}"#,
            )
            .expect("completed chunk parses")
        {
            Some(MessageStreamEvent::Usage(u)) => {
                assert_eq!(u.usage.prompt_tokens, 10);
                assert_eq!(u.usage.completion_tokens, 5);
                assert_eq!(u.usage.total_tokens, 15);
                assert_eq!(u.usage.reasoning_tokens, Some(2));
                assert_eq!(u.usage.cache_read_tokens, Some(3));
            }
            other => panic!("expected Usage, got {other:?}"),
        }

        match formatter
            .parse_stream_chunk(r#"{"type":"response.completed"}"#)
            .expect("completed without usage parses")
        {
            Some(MessageStreamEvent::End(_)) => {}
            other => panic!("expected End, got {other:?}"),
        }

        assert!(formatter.parse_stream_chunk("").unwrap().is_none());
        assert!(formatter
            .parse_stream_chunk(r#"{"type":"unknown"}"#)
            .unwrap()
            .is_none());
    }

    #[test]
    fn multiple_call_deltas_get_distinct_indices() {
        let formatter = OpenaiResponseFormatter::new();
        let chunk = |id: &str| {
            format!(
                r#"{{"type":"response.function_call_arguments.delta","item_id":"{id}","delta":"x"}}"#
            )
        };
        let i1 = match formatter
            .parse_stream_chunk(&chunk("fc_a"))
            .unwrap()
            .unwrap()
        {
            MessageStreamEvent::ToolCallDelta(d) => d.index,
            other => panic!("expected ToolCallDelta, got {other:?}"),
        };
        let i2 = match formatter
            .parse_stream_chunk(&chunk("fc_b"))
            .unwrap()
            .unwrap()
        {
            MessageStreamEvent::ToolCallDelta(d) => d.index,
            other => panic!("expected ToolCallDelta, got {other:?}"),
        };
        assert_ne!(i1, i2);
        // Repeating the same item_id keeps its index.
        let again = match formatter
            .parse_stream_chunk(&chunk("fc_a"))
            .unwrap()
            .unwrap()
        {
            MessageStreamEvent::ToolCallDelta(d) => d.index,
            other => panic!("expected ToolCallDelta, got {other:?}"),
        };
        assert_eq!(again, i1);
    }

    #[test]
    fn parse_response_extracts_text_output_and_usage() {
        let formatter = OpenaiResponseFormatter::new();
        let body = r#"{
            "id": "resp_1",
            "object": "response",
            "model": "gpt-4o",
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "the answer"}]
            }],
            "usage": {
                "input_tokens": 12,
                "output_tokens": 8,
                "total_tokens": 20,
                "input_tokens_details": {"cached_tokens": 5}
            }
        }"#;
        let result = formatter
            .parse_response(
                body,
                &LlmRequest {
                    profile_id: "p".to_string(),
                    messages: Vec::new(),
                    parameters: None,
                    tools: None,
                    tool_call_format: None,
                    locked_tool_call_format: None,
                    violation_policy: None,
                    execution_id: None,
                    stream: None,
                    dead_loop_detection: None,
                    protocol_auto_converted: None,
                },
            )
            .unwrap();
        assert_eq!(result.content.as_deref(), Some("the answer"));
        assert_eq!(result.finish_reason.as_deref(), Some("completed"));
        assert_eq!(result.usage.as_ref().unwrap().total_tokens, 20);
        assert_eq!(result.usage.as_ref().unwrap().cache_read_tokens, Some(5));
        assert_eq!(result.model, "gpt-4o");
    }

    #[test]
    fn parse_response_extracts_function_calls() {
        let formatter = OpenaiResponseFormatter::new();
        let body = r#"{
            "id": "resp_2",
            "object": "response",
            "model": "gpt-4o",
            "status": "completed",
            "output": [
                {"type": "message", "content": [{"type": "output_text", "text": ""}]},
                {"type": "function_call", "id": "fc_7", "call_id": "call_7", "name": "get_weather", "arguments": "{\"city\":\"Beijing\"}"}
            ]
        }"#;
        let result = formatter
            .parse_response(
                body,
                &LlmRequest {
                    profile_id: "p".to_string(),
                    messages: Vec::new(),
                    parameters: None,
                    tools: None,
                    tool_call_format: None,
                    locked_tool_call_format: None,
                    violation_policy: None,
                    execution_id: None,
                    stream: None,
                    dead_loop_detection: None,
                    protocol_auto_converted: None,
                },
            )
            .unwrap();
        let calls = result.tool_calls.as_ref().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "fc_7");
        assert_eq!(calls[0].function.arguments, r#"{"city":"Beijing"}"#);
    }

    #[test]
    fn parse_response_tolerates_empty_output() {
        let formatter = OpenaiResponseFormatter::new();
        let body = r#"{"id":"resp_3","object":"response","model":"gpt-4o","status":"incomplete"}"#;
        let result = formatter
            .parse_response(
                body,
                &LlmRequest {
                    profile_id: "p".to_string(),
                    messages: Vec::new(),
                    parameters: None,
                    tools: None,
                    tool_call_format: None,
                    locked_tool_call_format: None,
                    violation_policy: None,
                    execution_id: None,
                    stream: None,
                    dead_loop_detection: None,
                    protocol_auto_converted: None,
                },
            )
            .unwrap();
        assert_eq!(result.content, None);
        assert_eq!(result.tool_calls, None);
        assert_eq!(result.usage, None);
    }
}
