//! Token event builders
//!
//! Constructs `BaseEvent`s for token usage warnings, limit exceedance, and
//! context compression. Metadata keys follow a fixed schema (see constants
//! below) so trigger conditions and external consumers can match on them.

use wf_types::events::{BaseEvent, EventType};

/// Metadata key: tokens used so far.
pub const KEY_TOKENS_USED: &str = "tokens_used";
/// Metadata key: configured token limit.
pub const KEY_TOKEN_LIMIT: &str = "token_limit";
/// Metadata key: percentage of the limit consumed (0-100).
pub const KEY_USAGE_PERCENTAGE: &str = "usage_percentage";
/// Metadata key: conversation message count (compression requested).
pub const KEY_MESSAGE_COUNT: &str = "message_count";
/// Metadata key: summary produced by compression.
pub const KEY_SUMMARY: &str = "summary";
/// Metadata key: token count after compression.
pub const KEY_TOKENS_AFTER: &str = "tokens_after";
/// Metadata key: name of the message array targeted by compression
/// (the workflow named context or the agent conversation identifier).
pub const KEY_TARGET_CONTEXT_ID: &str = "target_context_id";
/// Metadata key: serialized conversation messages carried by the compression
/// requested event (input side of the event-driven compression chain) and by
/// the compression completed event (the compressed array).
pub const KEY_MESSAGES: &str = "messages";

/// Default token warning threshold percentage of the configured limit.
pub const DEFAULT_TOKEN_WARNING_THRESHOLD: u32 = 80;

fn metadata(pairs: Vec<(&str, serde_json::Value)>) -> wf_types::Metadata {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn base_event(event_type: EventType, execution_id: &str, agent_loop_id: Option<&str>) -> BaseEvent {
    BaseEvent {
        id: wf_common::generate_id(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(execution_id.to_string()),
        agent_loop_id: agent_loop_id.map(|s| s.to_string()),
        metadata: None,
    }
}

/// Build a TOKEN_USAGE_WARNING event.
///
/// Emitted once per session when the usage percentage first crosses the
/// configured warning threshold.
pub fn build_token_usage_warning_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    tokens_used: u64,
    token_limit: u64,
    usage_percentage: f64,
) -> BaseEvent {
    let mut event = base_event(EventType::TokenUsageWarning, execution_id, agent_loop_id);
    event.metadata = Some(metadata(vec![
        (KEY_TOKENS_USED, serde_json::json!(tokens_used)),
        (KEY_TOKEN_LIMIT, serde_json::json!(token_limit)),
        (
            KEY_USAGE_PERCENTAGE,
            serde_json::json!((usage_percentage * 100.0).round() / 100.0),
        ),
    ]));
    event
}

/// Build a TOKEN_LIMIT_EXCEEDED event.
pub fn build_token_limit_exceeded_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    tokens_used: u64,
    token_limit: u64,
) -> BaseEvent {
    let mut event = base_event(EventType::TokenLimitExceeded, execution_id, agent_loop_id);
    event.metadata = Some(metadata(vec![
        (KEY_TOKENS_USED, serde_json::json!(tokens_used)),
        (KEY_TOKEN_LIMIT, serde_json::json!(token_limit)),
    ]));
    event
}

/// Build a CONTEXT_COMPRESSION_REQUESTED event.
///
/// Emitted when a named message array exceeds the configured token limit; a
/// context-compression trigger (predefined `context_compression_trigger`)
/// reacts by running an LLM summary workflow over that array. The event
/// always carries the target array name (`target_context_id`) and its message
/// snapshot (`messages`) so the trigger executor can reproduce the
/// conversation and write the compressed result back to the same array.
pub fn build_context_compression_requested_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    target_context_id: &str,
    tokens_used: u64,
    token_limit: u64,
    message_count: usize,
    messages: Option<&[wf_types::message::Message]>,
) -> BaseEvent {
    let mut event = base_event(
        EventType::ContextCompressionRequested,
        execution_id,
        agent_loop_id,
    );
    let mut pairs = vec![
        (KEY_TARGET_CONTEXT_ID, serde_json::json!(target_context_id)),
        (KEY_TOKENS_USED, serde_json::json!(tokens_used)),
        (KEY_TOKEN_LIMIT, serde_json::json!(token_limit)),
        (KEY_MESSAGE_COUNT, serde_json::json!(message_count)),
    ];
    if let Some(messages) = messages {
        if let Ok(value) = serde_json::to_value(messages) {
            pairs.push((KEY_MESSAGES, value));
        }
    }
    event.metadata = Some(metadata(pairs));
    event
}

/// Build a CONTEXT_COMPRESSION_COMPLETED event.
///
/// Carries the target array name and the compressed message array so any
/// consumer (the main workflow execution, an agent loop) can reproduce the
/// write-back without extra lookups.
pub fn build_context_compression_completed_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    target_context_id: &str,
    summary: Option<&str>,
    tokens_after: u64,
    messages: Option<&[wf_types::message::Message]>,
) -> BaseEvent {
    let mut event = base_event(
        EventType::ContextCompressionCompleted,
        execution_id,
        agent_loop_id,
    );
    let mut pairs = vec![
        (KEY_TARGET_CONTEXT_ID, serde_json::json!(target_context_id)),
        (KEY_TOKENS_AFTER, serde_json::json!(tokens_after)),
    ];
    if let Some(summary) = summary {
        pairs.push((KEY_SUMMARY, serde_json::json!(summary)));
    }
    if let Some(messages) = messages {
        if let Ok(value) = serde_json::to_value(messages) {
            pairs.push((KEY_MESSAGES, value));
        }
    }
    event.metadata = Some(metadata(pairs));
    event
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warning_event_metadata() {
        let event = build_token_usage_warning_event("exec-1", None, 900, 1000, 90.0);
        assert_eq!(event.r#type, EventType::TokenUsageWarning);
        assert_eq!(event.execution_id.as_deref(), Some("exec-1"));
        let meta = event.metadata.unwrap();
        assert_eq!(meta[KEY_TOKENS_USED], serde_json::json!(900));
        assert_eq!(meta[KEY_TOKEN_LIMIT], serde_json::json!(1000));
        assert_eq!(meta[KEY_USAGE_PERCENTAGE], serde_json::json!(90.0));
    }

    #[test]
    fn test_limit_exceeded_event() {
        let event = build_token_limit_exceeded_event("exec-1", Some("loop-1"), 1200, 1000);
        assert_eq!(event.r#type, EventType::TokenLimitExceeded);
        assert_eq!(event.agent_loop_id.as_deref(), Some("loop-1"));
    }

    #[test]
    fn test_compression_requested_event() {
        let event =
            build_context_compression_requested_event("exec-1", None, "chat", 1200, 1000, 42, None);
        assert_eq!(event.r#type, EventType::ContextCompressionRequested);
        let meta = event.metadata.unwrap();
        assert_eq!(meta[KEY_TARGET_CONTEXT_ID], serde_json::json!("chat"));
        assert_eq!(meta[KEY_MESSAGE_COUNT], serde_json::json!(42));
        assert!(!meta.contains_key(KEY_MESSAGES));

        // Messages payload is embedded when provided.
        let msg = wf_types::message::Message {
            id: wf_types::Id::new(),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text("hi".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        let event = build_context_compression_requested_event(
            "exec-1",
            None,
            "chat",
            1200,
            1000,
            1,
            Some(std::slice::from_ref(&msg)),
        );
        let meta = event.metadata.unwrap();
        let messages: Vec<wf_types::message::Message> =
            serde_json::from_value(meta[KEY_MESSAGES].clone()).unwrap();
        assert_eq!(messages, vec![msg]);
    }

    #[test]
    fn test_compression_completed_event() {
        let msg = wf_types::message::Message {
            id: wf_types::Id::new(),
            role: wf_types::message::MessageRole::Assistant,
            content: wf_types::message::MessageContentValue::Text("compressed".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        let event = build_context_compression_completed_event(
            "exec-1",
            None,
            "chat",
            Some("summary"),
            300,
            Some(std::slice::from_ref(&msg)),
        );
        assert_eq!(event.r#type, EventType::ContextCompressionCompleted);
        let meta = event.metadata.unwrap();
        assert_eq!(meta[KEY_TARGET_CONTEXT_ID], serde_json::json!("chat"));
        assert_eq!(meta[KEY_SUMMARY], serde_json::json!("summary"));
        assert_eq!(meta[KEY_TOKENS_AFTER], serde_json::json!(300));
        let messages: Vec<wf_types::message::Message> =
            serde_json::from_value(meta[KEY_MESSAGES].clone()).unwrap();
        assert_eq!(messages, vec![msg]);

        let no_summary =
            build_context_compression_completed_event("exec-1", None, "chat", None, 0, None);
        let meta = no_summary.metadata.unwrap();
        assert!(!meta.contains_key(KEY_SUMMARY));
        assert!(!meta.contains_key(KEY_MESSAGES));
    }
}
