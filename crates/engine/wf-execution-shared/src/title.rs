//! Screen-only title generation: one short title for a conversation.
//!
//! A title is a side-channel read of the stored conversation. It never
//! writes to the session log. Every failure (empty history, gateway error,
//! timeout, empty model answer) converges to `None`.

use std::time::Duration;

use wf_llm::LlmGateway;
use wf_types::llm::{LlmGenerationParams, LlmRequest, LlmThinkingConfig, ThinkingLevel};
use wf_types::message::{Message, MessageRole};

/// Upper bound of the cleaned title text.
pub const MAX_TITLE_CHARS: usize = 80;

/// Wall-clock bound of the side call.
pub const TITLE_TIMEOUT_MS: u64 = 15_000;

/// Low-temperature sampling for a deterministic short title.
pub const TITLE_TEMPERATURE: f64 = 0.2;

/// Instructions for the side call.
pub const TITLE_INSTRUCTIONS: &str = "Generate a short title (at most six words) for this conversation based on the user's first request. Reply with the title only, no quotes, no punctuation at the end.";

/// Build a compact transcript from the first few messages.
pub fn title_context(messages: &[Message]) -> Option<String> {
    if messages.is_empty() {
        return None;
    }
    let mut has_user = false;
    let mut lines: Vec<String> = Vec::new();
    for message in messages.iter().take(6) {
        let text = message.text_content();
        if text.trim().is_empty() {
            continue;
        }
        if message.role == MessageRole::User {
            has_user = true;
        }
        let trimmed = if text.len() > 300 {
            let mut s = text[..300].to_string();
            s.push('…');
            s
        } else {
            text.to_string()
        };
        lines.push(format!("{:?}: {}", message.role, trimmed));
    }
    if !has_user || lines.is_empty() {
        return None;
    }
    Some(lines.join("\n"))
}

/// Clean one raw model answer into a title, or `None` when unusable.
pub fn sanitize_title(raw: &str) -> Option<String> {
    let mut text = raw.trim().to_string();
    if text.is_empty() {
        return None;
    }
    // Strip quotes.
    if text.len() >= 2 {
        let bytes = text.as_bytes();
        let first = bytes[0] as char;
        let last = text.chars().last().unwrap_or(' ');
        if (first == '"' && last == '"')
            || (first == '\'' && last == '\'')
            || (first == '“' && last == '”')
        {
            text = text[1..text.len() - 1].trim().to_string();
        }
    }
    text = text.replace('\n', " ");
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.eq_ignore_ascii_case("<none>") {
        return None;
    }
    let chars: Vec<char> = collapsed.chars().collect();
    if chars.len() <= MAX_TITLE_CHARS {
        Some(collapsed)
    } else {
        let kept: String = chars[..MAX_TITLE_CHARS].iter().collect();
        Some(kept.trim_end().to_string())
    }
}

/// Build the side-call request.
pub fn build_title_request(profile_id: &str, context: &str) -> LlmRequest {
    let system = Message::system_text(TITLE_INSTRUCTIONS.to_string());
    let user = Message::user_text(context.to_string());
    LlmRequest {
        profile_id: profile_id.to_string(),
        messages: vec![system, user],
        parameters: None,
        generation: Some(LlmGenerationParams {
            temperature: Some(TITLE_TEMPERATURE),
            thinking: Some(LlmThinkingConfig {
                level: Some(ThinkingLevel::Low),
                ..Default::default()
            }),
            ..Default::default()
        }),
        tools: None,
        tool_call_protocol: None,
        locked_tool_call_protocol: None,
        violation_policy: None,
        execution_id: Some("title".to_string()),
        stream: None,
        dead_loop_detection: None,
        protocol_auto_converted: None,
        timeout_ms: Some(TITLE_TIMEOUT_MS),
        prompt_caching: None,
        cached_content: None,
        max_retries: None,
        retry_delay_ms: None,
        extra_headers: None,
    }
}

/// Run the side call and clean its answer.
pub async fn generate_title_text(
    gateway: &LlmGateway,
    profile_id: &str,
    context: &str,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> Option<String> {
    if profile_id.trim().is_empty() || context.trim().is_empty() {
        return None;
    }
    let request = build_title_request(profile_id, context);
    let result = tokio::time::timeout(
        Duration::from_millis(TITLE_TIMEOUT_MS),
        gateway.generate(&request, cancel),
    )
    .await;
    let response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            tracing::debug!(target: "wf_title", error = %error, "title side call failed");
            return None;
        }
        Err(_) => {
            tracing::debug!(target: "wf_title", "title side call timed out");
            return None;
        }
    };
    let raw = response
        .content
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| response.message.text_content());
    if raw.trim().is_empty() {
        return None;
    }
    sanitize_title(&raw)
}

/// Full pipeline.
pub async fn request_title(
    gateway: &LlmGateway,
    profile_id: &str,
    messages: &[Message],
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> Option<String> {
    let context = title_context(messages)?;
    generate_title_text(gateway, profile_id, &context, cancel).await
}
