//! Token counting for LLM requests
//!
//! Combination layer built on top of [`TokenEstimator`]: counts messages,
//! tools, and multimodal content. Prefers the provider's count-tokens API
//! when available, otherwise falls back to local estimation (API-first,
//! mirroring the TypeScript `getTokenUsage`).

use crate::client::LlmClientImpl;
use crate::error::{LlmError, LlmResult};
use crate::token_estimation::{estimate_tokens, TokenEstimator, MESSAGE_OVERHEAD_TOKENS};
use wf_types::llm::{LlmRequest, TokenCountResult};
use wf_types::message::{Message, MessageContent, MessageContentValue};

/// Estimate tokens for a single message: content (text / rich blocks),
/// thinking, tool calls, plus a fixed metadata overhead per message.
pub fn estimate_message_tokens(msg: &Message) -> u32 {
    let mut total = 0u32;

    match &msg.content {
        MessageContentValue::Text(text) => total += estimate_tokens(text) as u32,
        MessageContentValue::Rich(blocks) => {
            for block in blocks {
                total += match block {
                    MessageContent::Text { text } => estimate_tokens(text) as u32,
                    MessageContent::Thinking { thinking, .. } => estimate_tokens(thinking) as u32,
                    MessageContent::ToolUse { tool_use } => {
                        let json = serde_json::to_string(&tool_use.input).unwrap_or_default();
                        estimate_tokens(&json) as u32
                    }
                    MessageContent::ToolResult { tool_result } => {
                        estimate_tokens(&tool_result.content) as u32
                    }
                    MessageContent::ImageUrl { image_url } => estimate_image_tokens(&image_url.url),
                };
            }
        }
    }

    if let Some(thinking) = &msg.thinking {
        total += estimate_tokens(thinking) as u32;
    }

    if let Some(tool_calls) = &msg.tool_calls {
        if let Ok(json) = serde_json::to_string(tool_calls) {
            total += estimate_tokens(&json) as u32;
        }
    }

    total + MESSAGE_OVERHEAD_TOKENS
}

/// Estimate tokens for a full LLM request: messages + tool declarations.
pub fn estimate_request_tokens(request: &LlmRequest) -> u32 {
    let mut total = 0u32;

    for msg in &request.messages {
        total += estimate_message_tokens(msg);
    }

    if let Some(tools) = &request.tools {
        for tool in tools {
            total += estimate_tokens(&tool.name) as u32;
            total += estimate_tokens(&tool.description) as u32;
            if let Some(params) = &tool.parameters {
                if let Ok(json) = serde_json::to_string(params) {
                    total += estimate_tokens(&json) as u32;
                }
            }
        }
    }

    total
}

/// Estimate tokens for an image reference.
///
/// Mirrors the TypeScript `estimateImageTokens` heuristic:
/// - data URI: derive pixel count from base64 length, `ceil(pixels/750) + 200`
/// - remote URL: conservative fixed estimate of 170 tokens
pub fn estimate_image_tokens(url_or_data_uri: &str) -> u32 {
    if let Some(rest) = url_or_data_uri.strip_prefix("data:") {
        let b64 = if let Some(semicolon) = rest.find(';') {
            rest[semicolon + 1..]
                .strip_prefix("base64,")
                .unwrap_or(&rest[semicolon + 1..])
        } else {
            rest
        };
        // Raw bytes ≈ 3/4 of base64 length; ~3 bytes per pixel (RGB)
        let bytes = b64.len() as f64 * 3.0 / 4.0;
        let pixels = bytes / 3.0;
        (pixels / 750.0).ceil() as u32 + 200
    } else {
        170
    }
}

/// Execute token counting for a request: provider API first, local
/// estimation as fallback.
pub(crate) async fn count_tokens_client(
    client_impl: &LlmClientImpl,
    request: &LlmRequest,
) -> LlmResult<TokenCountResult> {
    if let Some(http_request) = client_impl
        .formatter
        .build_count_tokens_request(request, &client_impl.profile)?
    {
        let timeout_dur = client_impl.build_timeout();
        let response = tokio::time::timeout(timeout_dur, client_impl.client.execute(http_request))
            .await
            .map_err(|_| LlmError::Timeout(timeout_dur.as_millis() as u64))?
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout(timeout_dur.as_millis() as u64)
                } else {
                    LlmError::HttpError(e)
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmClientImpl::map_http_error(
                status,
                &body,
                timeout_dur.as_millis() as u64,
            ));
        }

        let body = response.text().await?;
        let json: serde_json::Value = serde_json::from_str(&body)?;
        let input_tokens = json
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        Ok(TokenCountResult {
            input_tokens,
            raw: Some(json),
        })
    } else {
        // Fallback to local estimation
        let estimated = estimate_request_tokens(request);
        Ok(TokenCountResult {
            input_tokens: estimated,
            raw: None,
        })
    }
}

/// Estimate the token count of the current conversation messages.
pub fn estimate_messages(messages: &[Message]) -> u32 {
    messages.iter().map(estimate_message_tokens).sum()
}

/// Estimator used by counting helpers.
pub fn default_estimator() -> TokenEstimator {
    TokenEstimator::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::{
        ImageUrlContent, Message, MessageContent, MessageContentValue, MessageRole,
        ToolResultContent,
    };

    fn text_message(role: MessageRole, text: &str) -> Message {
        Message {
            id: "id".to_string(),
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

    #[test]
    fn test_estimate_simple_message() {
        let msg = text_message(MessageRole::User, "Hello");
        // 5 * 0.25 = 1.25 -> 1 + 4 overhead = 5
        assert_eq!(estimate_message_tokens(&msg), 5);
    }

    #[test]
    fn test_estimate_cjk_message() {
        let msg = text_message(MessageRole::User, "你好世界");
        // 4 CJK = 4 + 4 overhead = 8
        assert_eq!(estimate_message_tokens(&msg), 8);
    }

    #[test]
    fn test_estimate_message_with_thinking() {
        let mut msg = text_message(MessageRole::Assistant, "result");
        msg.thinking = Some("思考过程".to_string());
        // content "result" 6 * 0.25 = 1.5 -> 2 + thinking 4 CJK = 4 + 4 overhead = 10
        assert_eq!(estimate_message_tokens(&msg), 10);
    }

    #[test]
    fn test_estimate_message_with_tool_calls() {
        let mut msg = text_message(MessageRole::Assistant, "");
        msg.tool_calls = Some(vec![wf_types::message::LlmToolCall {
            id: "call_1".to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: "get_weather".to_string(),
                arguments: r#"{"city":"Beijing"}"#.to_string(),
            },
        }]);
        let total = estimate_message_tokens(&msg);
        // JSON estimate + 4 overhead; must be at least overhead
        assert!(total >= 4, "tokens: {}", total);
    }

    #[test]
    fn test_estimate_rich_content() {
        let msg = Message {
            id: "id".to_string(),
            role: MessageRole::User,
            content: MessageContentValue::Rich(vec![
                MessageContent::Text {
                    text: "look at this".to_string(),
                },
                MessageContent::ImageUrl {
                    image_url: ImageUrlContent {
                        url: "https://example.com/img.png".to_string(),
                        detail: None,
                    },
                },
            ]),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        let total = estimate_message_tokens(&msg);
        // text ~3 + image 170 + 4 overhead
        assert!(total >= 170, "tokens: {}", total);
    }

    #[test]
    fn test_estimate_rich_tool_result() {
        let msg = Message {
            id: "id".to_string(),
            role: MessageRole::Tool,
            content: MessageContentValue::Rich(vec![MessageContent::ToolResult {
                tool_result: ToolResultContent {
                    tool_use_id: "call_1".to_string(),
                    content: "the weather is sunny".to_string(),
                    is_error: None,
                },
            }]),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        let total = estimate_message_tokens(&msg);
        assert!(total >= 4, "tokens: {}", total);
    }

    #[test]
    fn test_estimate_image_tokens_url() {
        assert_eq!(estimate_image_tokens("https://example.com/a.png"), 170);
    }

    #[test]
    fn test_estimate_image_tokens_data_uri() {
        // 900 pixels -> ceil(900/750) + 200 = 202
        // b64 length for 900*3 bytes = 2700 bytes -> ceil(2700/3)=900 px
        // base64 of 2700 bytes is 3600 chars
        let uri = format!("data:image/png;base64,{}", "A".repeat(3600));
        assert_eq!(estimate_image_tokens(&uri), 202);
    }

    #[test]
    fn test_estimate_request_tokens() {
        let req = LlmRequest {
            profile_id: "default".to_string(),
            messages: vec![
                text_message(MessageRole::System, "You are a helper"),
                text_message(MessageRole::User, "你好"),
            ],
            parameters: None,
            tools: None,
            tool_call_format: None,
            locked_tool_call_format: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        };
        // system: 13 letters * 0.25 + 3 spaces * 0.5 = 4.75 -> 5 + 4 = 9;
        // user: 2 + 4 = 6; total 15
        assert_eq!(estimate_request_tokens(&req), 15);
    }

    #[test]
    fn test_estimate_messages_empty() {
        assert_eq!(estimate_messages(&[]), 0);
    }
}
