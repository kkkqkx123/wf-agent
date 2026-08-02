use super::shared;
use super::LlmFormatter;
use crate::error::LlmResult;
use reqwest::Method;
use wf_types::llm::{LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent};
use wf_types::tool::Tool;

pub struct OpenaiChatFormatter {
    base_url: String,
}

impl Default for OpenaiChatFormatter {
    fn default() -> Self {
        Self::new()
    }
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

    /// Build the request body (separated from the HTTP layer for testability).
    fn build_body(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<serde_json::Value> {
        let use_text_mode = shared::is_text_mode(request);

        let messages = if use_text_mode {
            let (_, filtered) = crate::tool_format::extract_system_message(&request.messages);
            let content = shared::text_mode_system_content(request);
            let mut converted = shared::convert_openai_messages(&filtered);
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
            let include_usage = profile
                .stream_options
                .as_ref()
                .and_then(|s| s.include_usage)
                .unwrap_or(true);
            body["stream_options"] = serde_json::json!({"include_usage": include_usage});
        }

        shared::apply_custom_body(&mut body, profile);

        Ok(body)
    }
}

impl LlmFormatter for OpenaiChatFormatter {
    fn build_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<reqwest::Request> {
        let url = format!(
            "{}/chat/completions",
            profile.base_url.as_deref().unwrap_or(&self.base_url)
        );

        let body = self.build_body(request, profile)?;

        let mut req_builder = reqwest::Client::new()
            .request(Method::POST, &url)
            .header("Content-Type", "application/json")
            .json(&body);

        req_builder = shared::apply_auth_and_headers(req_builder, profile, "bearer");

        req_builder
            .build()
            .map_err(crate::error::LlmError::HttpError)
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
    use wf_types::llm::{LlmStreamOptions, ToolCallFormat};
    use wf_types::message::{Message, MessageContentValue, MessageRole};

    fn text_msg(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn profile() -> LlmProfile {
        LlmProfile {
            id: "p1".to_string(),
            name: "test".to_string(),
            provider: wf_types::llm::LlmProvider::OpenaiChat,
            model: "gpt-4o".to_string(),
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
        }
    }

    fn request_with_format(format: ToolCallFormat) -> LlmRequest {
        LlmRequest {
            profile_id: "p1".to_string(),
            messages: vec![
                text_msg(MessageRole::System, "You are a helper"),
                text_msg(MessageRole::User, "What is the weather?"),
            ],
            parameters: None,
            tools: Some(vec![serde_json::from_value(serde_json::json!({
                "id": wf_types::Id::new(),
                "name": "get_weather",
                "description": "Get weather",
                "tool_type": "built_in",
                "parameters": {
                    "type": "object",
                    "properties": {"city": {"name": "city", "value": null, "type": "string"}},
                    "required": ["city"]
                },
                "enabled": true
            }))
            .unwrap()]),
            tool_call_format: Some(format),
            locked_tool_call_format: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        }
    }

    #[test]
    fn text_mode_injects_templates_and_skips_native_tools() {
        let formatter = OpenaiChatFormatter::new();
        let body = formatter
            .build_body(&request_with_format(ToolCallFormat::Xml), &profile())
            .unwrap();

        assert!(body.get("tools").is_none(), "text mode must not send tools");
        let system = body["messages"][0]["content"].as_str().unwrap();
        assert!(
            system.contains("get_weather"),
            "system must declare tools: {system}"
        );
        assert!(
            system.contains("<tool_use>"),
            "system must carry usage instructions"
        );
        assert!(system.contains("You are a helper"), "original system kept");
    }

    #[test]
    fn native_mode_sends_tools_and_keeps_original_system() {
        let formatter = OpenaiChatFormatter::new();
        let body = formatter
            .build_body(&request_with_format(ToolCallFormat::Native), &profile())
            .unwrap();

        assert!(body.get("tools").is_some());
        let system = body["messages"][0]["content"].as_str().unwrap();
        assert_eq!(system, "You are a helper");
    }

    #[test]
    fn text_mode_parse_extracts_xml_tool_calls() {
        let formatter = OpenaiChatFormatter::new();
        let body = r#"{"id":"1","model":"gpt-4o","choices":[{"message":{"role":"assistant","content":"<tool_use>\n  <tool_name>get_weather</tool_name>\n  <parameters>\n    <city>Beijing</city>\n  </parameters>\n</tool_use>"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;

        let result = formatter
            .parse_response(body, &request_with_format(ToolCallFormat::Xml))
            .unwrap();
        let calls = result.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "get_weather");
        assert!(calls[0].function.arguments.contains("Beijing"));
    }

    #[test]
    fn stream_options_from_profile() {
        let mut p = profile();
        p.stream_options = Some(LlmStreamOptions {
            include_usage: Some(false),
        });
        let mut req = request_with_format(ToolCallFormat::Native);
        req.stream = Some(true);
        let formatter = OpenaiChatFormatter::new();
        let body = formatter.build_body(&req, &p).unwrap();
        assert_eq!(
            body["stream_options"]["include_usage"],
            serde_json::json!(false)
        );
    }

    #[test]
    fn custom_body_is_deep_merged() {
        let mut p = profile();
        p.custom_body = Some(serde_json::json!({"custom_field": {"nested": 1}}));
        let formatter = OpenaiChatFormatter::new();
        let body = formatter
            .build_body(&request_with_format(ToolCallFormat::Native), &p)
            .unwrap();
        assert_eq!(body["custom_field"]["nested"], serde_json::json!(1));
    }
}
