//! Screen-only recap: one short line recalling where a turn left off.
//!
//! A recap is a side-channel read of the stored conversation. It never
//! writes to the session log, never enters the next request and never
//! changes the turn outcome. Every failure (no stable transcript, gateway
//! error, timeout, empty model answer, rejected cleanup) converges to
//! `None`: the caller simply shows nothing.

use std::time::Duration;

use wf_llm::LlmGateway;
use wf_types::llm::{LlmGenerationParams, LlmRequest, LlmThinkingConfig, ThinkingLevel};
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};

/// Upper bound of the cleaned recap text. Longer model answers are cut at a
/// sentence boundary, otherwise an ellipsis is appended.
pub const MAX_RECAP_CHARS: usize = 240;

/// Upper bound of the projected transcript fed to the side call. The
/// projection fills from the tail so recent turns survive truncation.
pub const MAX_CONTEXT_CHARS: usize = 16_384;

/// Wall-clock bound of the side call. The timeout only ends the bypass; the
/// main turn already completed and is never affected.
pub const RECAP_TIMEOUT_MS: u64 = 25_000;

/// Low-temperature sampling for a deterministic one-liner.
pub const RECAP_TEMPERATURE: f64 = 0.2;

/// Instructions for the side call. The prompt must keep the `Write a recap`
/// lead so the existing acceptance probe (request count + prompt marker)
/// stays meaningful across refactors.
pub const RECAP_INSTRUCTIONS: &str = "Write a recap of the conversation so far in one short sentence for the human who just looked back at the screen. Say what was just done and what waits next. If there is nothing worth saying, reply with <none> and nothing else.";

/// Build the transcript fed to the side call, or `None` when the turn is not
/// worth asking about. `None` means the caller must not issue a model
/// request at all: no stable tail, no user turn, or nothing but whitespace.
pub fn stable_transcript(messages: &[Message]) -> Option<String> {
    if messages.is_empty() {
        return None;
    }
    let last = messages.last()?;
    if !is_stable_assistant(last) {
        return None;
    }
    let mut has_user = false;
    let mut lines: Vec<String> = Vec::new();
    let mut budget = MAX_CONTEXT_CHARS;
    for message in messages.iter().rev() {
        if message.role == MessageRole::User {
            has_user = true;
        }
        let Some(line) = project_message(message) else {
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        let cost = line.len().min(MAX_CONTEXT_CHARS);
        if lines.is_empty() {
            // The stable tail always survives: a single long line is
            // truncated to the budget instead of dropping the recap,
            // then earlier turns still fill any remaining budget.
            let mut kept = line;
            if kept.len() > budget {
                kept.truncate(budget);
            }
            let used = kept.len().min(budget);
            budget = budget.saturating_sub(used);
            lines.push(kept);
            if budget == 0 {
                break;
            }
            continue;
        }
        if cost + 1 > budget {
            break;
        }
        budget = budget.saturating_sub(cost + 1);
        lines.push(line);
        if budget == 0 {
            break;
        }
    }
    if !has_user || lines.is_empty() {
        return None;
    }
    lines.reverse();
    let transcript = lines.join("\n");
    if transcript.trim().is_empty() {
        return None;
    }
    Some(transcript)
}

/// Whether a message is a stable assistant turn: plain assistant text with
/// no tool calls and non-empty content. Tool calls, tool results and empty
/// replies are never recap sources.
fn is_stable_assistant(message: &Message) -> bool {
    if message.role != MessageRole::Assistant {
        return false;
    }
    if message
        .tool_calls
        .as_ref()
        .is_some_and(|calls| !calls.is_empty())
    {
        return false;
    }
    if let MessageContentValue::Rich(blocks) = &message.content {
        for block in blocks {
            if matches!(
                block,
                MessageContent::ToolUse { .. }
                    | MessageContent::ToolResult { .. }
                    | MessageContent::Thinking { .. }
                    | MessageContent::RedactedThinking { .. }
            ) {
                return false;
            }
        }
    }
    !message.text_content().trim().is_empty()
}

/// Project one message onto a JSON line. Injected text stays escaped inside
/// the JSON string: it is data for the side call, never parsed structure.
/// Returns `None` for content that carries no recap signal (system notes are
/// skipped, empty texts are skipped).
fn project_message(message: &Message) -> Option<String> {
    let role = match message.role {
        MessageRole::System => return None,
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    };
    let mut text = message.text_content();
    if text.trim().is_empty() {
        // Tool messages may still carry a call summary even with empty text.
        if message.role != MessageRole::Tool {
            return None;
        }
        text = describe_tool_message(message);
        if text.trim().is_empty() {
            return None;
        }
    }
    if text.len() > 2000 {
        text.truncate(2000);
    }
    // Tool arguments travel redacted: secrets are masked, bulky payloads
    // are replaced by their byte size, paths stay for orientation.
    let record = if message.role == MessageRole::Tool {
        let args = message
            .tool_calls
            .as_ref()
            .map(|calls| {
                calls
                    .iter()
                    .map(|call| {
                        serde_json::json!({
                            "name": call.function.name,
                            "arguments": redact_arguments_text(&call.function.arguments),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        serde_json::json!({"role": role, "text": text, "tool": message.tool_name, "calls": args})
    } else if let Some(calls) = message
        .tool_calls
        .as_ref()
        .filter(|calls| !calls.is_empty())
    {
        let args = calls
            .iter()
            .map(|call| {
                serde_json::json!({
                    "name": call.function.name,
                    "arguments": redact_arguments_text(&call.function.arguments),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({"role": role, "text": text, "calls": args})
    } else {
        serde_json::json!({"role": role, "text": text})
    };
    serde_json::to_string(&record).ok()
}

/// Human-readable fallback for tool messages with no text content.
fn describe_tool_message(message: &Message) -> String {
    if let Some(name) = message.tool_name.as_deref() {
        if let Some(id) = message.tool_call_id.as_deref() {
            return format!("tool {name} result for {id}");
        }
        return format!("tool {name} result");
    }
    if let Some(id) = message.tool_call_id.as_deref() {
        return format!("tool result for {id}");
    }
    String::new()
}

/// Redact a raw tool-argument JSON string for the transcript. Unparseable
/// arguments are dropped to an omission marker rather than leaking raw text.
fn redact_arguments_text(raw: &str) -> serde_json::Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return serde_json::Value::Null;
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => redact_arguments(&value),
        Err(_) => serde_json::json!({"<omitted>": format!("{} bytes", raw.len())}),
    }
}

/// Redact a tool-argument value: secret keys are masked, bulky content keys
/// become byte-size markers, path-like keys are kept truncated.
pub fn redact_arguments(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, val) in map {
                let lower = key.to_lowercase();
                if lower.contains("api_key")
                    || lower.contains("apikey")
                    || lower.contains("token")
                    || lower.contains("password")
                    || lower.contains("secret")
                    || lower.contains("auth")
                {
                    out.insert(
                        key.clone(),
                        serde_json::Value::String("<redacted>".to_string()),
                    );
                } else if lower.contains("content")
                    || lower.contains("command")
                    || lower.contains("patch")
                    || lower.contains("script")
                    || lower.contains("diff")
                    || lower == "body"
                {
                    let bytes = serde_json::to_string(val).map(|s| s.len()).unwrap_or(0);
                    out.insert(
                        key.clone(),
                        serde_json::Value::String(format!("<omitted {bytes} bytes>")),
                    );
                } else if lower.contains("path") || lower.contains("url") || lower.contains("file")
                {
                    match val {
                        serde_json::Value::String(s) => {
                            let mut kept = s.clone();
                            if kept.len() > 180 {
                                kept.truncate(180);
                            }
                            out.insert(key.clone(), serde_json::Value::String(kept));
                        }
                        _ => {
                            out.insert(key.clone(), redact_arguments(val));
                        }
                    }
                } else {
                    out.insert(key.clone(), redact_arguments(val));
                }
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(redact_arguments).collect())
        }
        _ => value.clone(),
    }
}

/// Clean one raw model answer into a single display line, or `None` when
/// there is nothing worth showing. The rules mirror the reference
/// implementation: strip fences and emphasis, drop the model's own prefix,
/// treat `<none>` as silence, collapse to one line and cut overlong answers
/// at a sentence boundary.
pub fn sanitize_recap(raw: &str) -> Option<String> {
    let mut text = raw.trim().to_string();
    if text.is_empty() {
        return None;
    }
    // Strip markdown fences (with or without a language tag).
    if text.starts_with("```") {
        if let Some(end) = text.rfind("```") {
            if end > 0 {
                let inner = text[3..end].trim();
                // Drop a leading language tag line when present.
                let inner = inner
                    .split_once('\n')
                    .map(|(first, rest)| {
                        if first.trim().chars().all(|c| c.is_alphanumeric())
                            && !rest.trim().is_empty()
                        {
                            rest.trim().to_string()
                        } else {
                            inner.to_string()
                        }
                    })
                    .unwrap_or_else(|| inner.to_string());
                text = inner;
            }
        }
    }
    text = text.replace('*', "");
    let mut collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed = collapsed.trim().to_string();
    if collapsed.is_empty() {
        return None;
    }
    // Drop the model's own label prefix when it echoes one.
    for prefix in ["recap:", "Recap:", "RECAP:", "回顾:", "回顾："] {
        if let Some(rest) = collapsed.strip_prefix(prefix) {
            collapsed = rest.trim().to_string();
            break;
        }
        // Also accept the label with a bullet prefix.
        for bullet in ["※ ", "* ", "- "] {
            if let Some(after) = collapsed.strip_prefix(bullet) {
                if let Some(rest) = after.trim_start().strip_prefix(prefix) {
                    collapsed = rest.trim().to_string();
                    break;
                }
            }
        }
    }
    // Strip wrapping quotes.
    if collapsed.len() >= 2 {
        let bytes = collapsed.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[collapsed.len() - 1] as char;
        if (first == '"' && last == '"')
            || (first == '\'' && last == '\'')
            || (first == '“' && last == '”')
        {
            collapsed = collapsed[1..collapsed.len() - 1].trim().to_string();
        }
    }
    if collapsed.trim().is_empty() {
        return None;
    }
    if collapsed.trim().eq_ignore_ascii_case("<none>") || collapsed.trim() == "无" {
        return None;
    }
    // Collapse any remaining newlines (already single-spaced above) and cut.
    let chars: Vec<char> = collapsed.chars().collect();
    if chars.len() <= MAX_RECAP_CHARS {
        return Some(collapsed);
    }
    let cut = MAX_RECAP_CHARS;
    // Prefer a sentence boundary before the limit (both scripts).
    let mut boundary: Option<usize> = None;
    for (idx, ch) in chars.iter().enumerate().take(MAX_RECAP_CHARS) {
        if matches!(*ch, '.' | '!' | '?' | '。' | '！' | '？') {
            boundary = Some(idx + 1);
        }
    }
    if let Some(end) = boundary {
        let kept: String = chars[..end].iter().collect();
        let kept = kept.trim().to_string();
        if !kept.is_empty() {
            return Some(kept);
        }
    }
    let kept: String = chars[..cut].iter().collect();
    let kept = kept.trim_end().to_string();
    Some(format!("{kept}…"))
}

/// Build the tool-free side-call request for a projected transcript. The
/// caller owns the profile: no fallback to a lightweight profile happens
/// here, so a missing profile is a caller error, not a silent model swap.
pub fn build_recap_request(profile_id: &str, transcript: &str) -> LlmRequest {
    let system = Message::system_text(RECAP_INSTRUCTIONS.to_string());
    let user = Message::user_text(transcript.to_string());
    LlmRequest {
        profile_id: profile_id.to_string(),
        messages: vec![system, user],
        parameters: None,
        generation: Some(LlmGenerationParams {
            temperature: Some(RECAP_TEMPERATURE),
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
        execution_id: Some("recap".to_string()),
        stream: None,
        dead_loop_detection: None,
        protocol_auto_converted: None,
        timeout_ms: Some(RECAP_TIMEOUT_MS),
        prompt_caching: None,
        cached_content: None,
        max_retries: None,
        retry_delay_ms: None,
        extra_headers: None,
    }
}

/// Run the side call and clean its answer. Any transport failure, timeout,
/// cancellation or empty answer converges to `None`.
pub async fn generate_recap_text(
    gateway: &LlmGateway,
    profile_id: &str,
    transcript: &str,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> Option<String> {
    if profile_id.trim().is_empty() || transcript.trim().is_empty() {
        return None;
    }
    let request = build_recap_request(profile_id, transcript);
    let result = tokio::time::timeout(
        Duration::from_millis(RECAP_TIMEOUT_MS),
        gateway.generate(&request, cancel),
    )
    .await;
    let response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            tracing::debug!(target: "wf_recap", error = %error, "recap side call failed");
            return None;
        }
        Err(_) => {
            tracing::debug!(target: "wf_recap", "recap side call timed out");
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
    sanitize_recap(&raw)
}

/// Full recap pipeline: project, ask once, clean. Returns `None` without
/// issuing a model request when the transcript is not worth asking about.
pub async fn request_recap(
    gateway: &LlmGateway,
    profile_id: &str,
    messages: &[Message],
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> Option<String> {
    let transcript = stable_transcript(messages)?;
    generate_recap_text(gateway, profile_id, &transcript, cancel).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_types::message::{LlmFunctionCall, LlmToolCall, MessageContentValue, MessageRole};

    fn user_message(text: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        }
    }

    fn assistant_message(text: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        }
    }

    fn mock_profile(id: &str) -> wf_types::llm::LlmProfile {
        wf_types::llm::LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            format: wf_types::llm::LlmFormat::OpenaiChat,
            provider_id: None,
            model: "mock-model".to_string(),
            api_key: Some("sk-test".into()),
            base_url: None,
            parameters: None,
            generation: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_protocol: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
            proxy: None,
            no_proxy: None,
            circuit_breaker: None,
            prompt_caching: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        }
    }

    #[test]
    fn a_recap_is_one_clean_line() {
        let out = sanitize_recap("```\n**recap:** \"Did the thing.\"\n```").expect("recap shown");
        assert!(!out.contains('\n'));
        assert!(!out.contains('*'));
        assert!(!out.contains("```"));
        assert!(out.chars().count() <= MAX_RECAP_CHARS);
    }

    #[test]
    fn nothing_to_say_is_no_recap() {
        assert_eq!(sanitize_recap("<none>"), None);
        assert_eq!(sanitize_recap("  <NONE>  "), None);
        assert_eq!(sanitize_recap(""), None);
        assert_eq!(sanitize_recap("   "), None);
    }

    #[test]
    fn a_long_one_is_cut_at_a_sentence_end() {
        let first = "First sentence is short. ";
        let second = "Second sentence carries on. ";
        let filler: String = "word ".repeat(100);
        let raw = format!("{first}{second}{filler}");
        let out = sanitize_recap(&raw).expect("long recap still shown");
        assert!(out.chars().count() <= MAX_RECAP_CHARS + 1);
        assert!(
            out.ends_with('.') || out.ends_with('。') || out.ends_with('…'),
            "long recap ends cleanly: {out}"
        );
        assert!(out.contains("First sentence"));
    }

    #[test]
    fn unstable_tail_has_no_projection() {
        // Last message is a tool call: not a stable assistant turn.
        let tool_call = LlmToolCall {
            id: "call-1".to_string(),
            r#type: "function".to_string(),
            function: LlmFunctionCall {
                name: "read".to_string(),
                arguments: "{}".to_string(),
            },
            call_id: None,
            thought_signature: None,
        };
        let mut unstable = assistant_message("doing");
        unstable.tool_calls = Some(vec![tool_call]);
        assert_eq!(stable_transcript(&[user_message("hi"), unstable]), None);
        // Assistant-only history without a user turn is not worth asking.
        assert_eq!(stable_transcript(&[assistant_message("hello")]), None);
        assert_eq!(stable_transcript(&[]), None);
    }

    #[test]
    fn overlong_transcript_keeps_the_tail() {
        let mut messages = vec![user_message("first question")];
        let filler: String = "x ".repeat(9000);
        messages.push(assistant_message(&format!("long answer {filler}")));
        messages.push(user_message("follow up"));
        messages.push(assistant_message("final answer"));
        let transcript = stable_transcript(&messages).expect("tail kept");
        assert!(transcript.len() <= MAX_CONTEXT_CHARS + 256);
        assert!(transcript.contains("final answer"));
    }

    #[test]
    fn tool_arguments_are_redacted() {
        let redacted = redact_arguments(&serde_json::json!({
            "api_key": "sk-live-123",
            "content": "a very long body",
            "path": "/tmp/a/very/long/path/that/stays",
        }));
        assert_eq!(
            redacted["api_key"],
            serde_json::Value::String("<redacted>".to_string())
        );
        assert!(redacted["content"]
            .as_str()
            .unwrap()
            .starts_with("<omitted"));
        assert!(redacted["path"].as_str().unwrap().contains("/tmp"));
    }

    #[tokio::test]
    async fn one_recap_is_one_model_request_with_the_recap_prompt() {
        let gateway = LlmGateway::new();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::text(
            "Wrapped up the auth route; next waits on your e2e call.",
        ));
        gateway.register_mock("mock-recap", mock.clone());
        gateway
            .register_profile(mock_profile("mock-recap"))
            .unwrap();

        let messages = vec![
            user_message("fix the /live auth route"),
            assistant_message("done, two tests pass; next is your e2e call"),
        ];
        let out = request_recap(&gateway, "mock-recap", &messages, None)
            .await
            .expect("recap produced");
        assert!(!out.is_empty());
        assert_eq!(mock.recorded_count(), 1);
        let recorded = mock.last_request().expect("request recorded");
        // The side call reuses the conversation profile: no extra provider.
        assert_eq!(recorded.profile_id, "mock-recap");
        assert!(recorded.tools.is_none(), "side call carries no tools");
        let prompt = serde_json::to_string(&recorded.messages).unwrap();
        assert!(
            prompt.contains("Write a recap"),
            "prompt carries the recap marker: {prompt}"
        );
    }

    #[tokio::test]
    async fn gateway_failure_converges_to_no_recap() {
        let gateway = LlmGateway::new();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script_error(wf_llm::LlmError::ProviderError {
            status: Some(500),
            message: "boom".to_string(),
            retry_after_ms: None,
        });
        gateway.register_mock("mock-fail", mock.clone());
        gateway.register_profile(mock_profile("mock-fail")).unwrap();

        let messages = vec![
            user_message("hi"),
            assistant_message("hello there, all good"),
        ];
        assert_eq!(
            request_recap(&gateway, "mock-fail", &messages, None).await,
            None
        );
    }

    #[tokio::test]
    async fn empty_projection_issues_no_request() {
        let gateway = LlmGateway::new();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::text("unused"));
        gateway.register_mock("mock-idle", mock.clone());
        gateway.register_profile(mock_profile("mock-idle")).unwrap();

        // No user turn: not worth asking, so no request leaves the process.
        let messages = vec![assistant_message("hello")];
        assert_eq!(
            request_recap(&gateway, "mock-idle", &messages, None).await,
            None
        );
        assert_eq!(mock.recorded_count(), 0);
    }
}
