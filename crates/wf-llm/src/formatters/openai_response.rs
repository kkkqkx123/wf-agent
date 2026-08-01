use super::LlmFormatter;
use crate::error::LlmResult;
use reqwest::Method;
use wf_types::llm::{LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent};
use wf_types::tool::Tool;

pub struct OpenaiResponseFormatter {
    base_url: String,
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
        }
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self { base_url }
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

        let mut body = serde_json::json!({
            "model": profile.model,
            "input": self.convert_messages(&request.messages),
        });

        if request.stream == Some(true) {
            body["stream"] = serde_json::json!(true);
        }

        let merged_params =
            crate::formatter_helpers::merge_parameters(profile, &request.parameters);
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
        if let Some(reasoning_effort) = merged_params.get("reasoning_effort") {
            body["reasoning_effort"] = reasoning_effort.clone();
        }
        if let Some(previous_response_id) = merged_params.get("previous_response_id") {
            body["previous_response_id"] = previous_response_id.clone();
        }
        if let Some(instructions) = merged_params.get("instructions") {
            body["instructions"] = instructions.clone();
        }

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

        req_builder
            .build()
            .map_err(crate::error::LlmError::HttpError)
    }

    fn parse_response(&self, body: &str) -> LlmResult<LlmResponseType> {
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
                reasoning_tokens: None,
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
                            text: delta.to_string(),
                        },
                    )));
                }
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
                                reasoning_tokens: None,
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
