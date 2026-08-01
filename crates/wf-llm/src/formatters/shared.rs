use crate::error::LlmResult;
use wf_types::llm::{LlmProfile, LlmResult as LlmResponseType, MessageStreamEvent};
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};

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

pub fn merge_and_apply_params(
    body: &mut serde_json::Value,
    profile: &LlmProfile,
    request_params: &Option<serde_json::Value>,
) {
    let merged_params = crate::formatter_helpers::merge_parameters(profile, request_params);
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
        body["stop"] = stop.clone();
    }
}

/// Parse an OpenAI-style usage object into token usage stats.
pub fn parse_openai_usage(u: &serde_json::Value) -> wf_types::llm::TokenUsageStats {
    let reasoning_tokens = u
        .get("completion_tokens_details")
        .and_then(|d| d.get("reasoning_tokens"))
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
                        text: content.to_string(),
                    },
                )));
            }
            if let Some(reasoning) = delta.get("reasoning_content").and_then(|r| r.as_str()) {
                return Ok(Some(MessageStreamEvent::ReasoningText(
                    wf_types::llm::MessageStreamReasoning {
                        reasoning: reasoning.to_string(),
                    },
                )));
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

pub fn add_auth_and_headers(
    req_builder: reqwest::RequestBuilder,
    profile: &LlmProfile,
    auth_type: &str,
) -> reqwest::RequestBuilder {
    let mut builder = req_builder;
    if let Some(api_key) = &profile.api_key {
        match auth_type {
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
    builder
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
}
