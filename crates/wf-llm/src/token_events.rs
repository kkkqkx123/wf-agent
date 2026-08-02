//! Token and stream event builders
//!
//! Constructs `BaseEvent`s for token usage warnings, limit exceedance,
//! context compression, and LLM stream termination (error / abort). Metadata
//! keys follow a fixed schema (see constants below) so trigger conditions and
//! external consumers can match on them.

use crate::error::LlmError;
use wf_types::events::{BaseEvent, EventType};
use wf_types::message::Message;

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
/// Metadata key: version of the target message array at emission time
/// (decision-track ledger version; emitters and listeners use it for
/// idempotency and versioned write-back).
pub const KEY_ARRAY_VERSION: &str = "array_version";
/// Metadata key: true when the compression request was forced by an API
/// context-length-exceeded error (safety-net path), false/absent when it
/// was emitted because the estimated array budget was exceeded.
pub const KEY_FORCED: &str = "forced";
/// Metadata key: number of transform_context-injected messages included in
/// the array budget check (informational; only present when > 0).
pub const KEY_INJECTED_MESSAGE_COUNT: &str = "injected_message_count";
/// Metadata key: serialized conversation messages carried by the compression
/// requested event (input side of the event-driven compression chain) and by
/// the compression completed event (the compressed array).
pub const KEY_MESSAGES: &str = "messages";
/// Metadata key: error message of a failed LLM stream.
pub const KEY_STREAM_ERROR: &str = "error";
/// Metadata key: abort reason of an aborted LLM stream.
pub const KEY_STREAM_ABORT_REASON: &str = "reason";
/// Metadata key: profile id the failing LLM stream was issued against.
pub const KEY_PROFILE_ID: &str = "profile_id";

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
/// Emitted when a named message array exceeds the configured token limit (or
/// when an API context-length-exceeded error forces it, see `forced`); a
/// context-compression trigger (predefined `context_compression_trigger`)
/// reacts by running an LLM summary workflow over that array. The event
/// always carries the target array name (`target_context_id`), its version
/// at emission time (`array_version`, decision-track idempotency), and its
/// message snapshot (`messages`) so the trigger executor can reproduce the
/// conversation and write the compressed result back to the same array.
///
/// `forced` distinguishes the two emission reasons: false (default) means
/// the estimated array budget was exceeded (decision track); true means the
/// provider rejected the actual request with a context-length error and the
/// event is a safety-net re-emission over the real request messages.
#[allow(clippy::too_many_arguments)]
pub fn build_context_compression_requested_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    target_context_id: &str,
    tokens_used: u64,
    token_limit: u64,
    message_count: usize,
    array_version: u64,
    forced: bool,
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
        (KEY_ARRAY_VERSION, serde_json::json!(array_version)),
    ];
    if forced {
        pairs.push((KEY_FORCED, serde_json::json!(true)));
    }
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
/// Carries the target array name, the compressed message array and the array
/// version the compression was produced from (the version of the matching
/// REQUESTED event), so any consumer (the main workflow execution, an agent
/// loop) can reproduce the write-back and version-check it without extra
/// lookups: an array that moved past `array_version` must discard the
/// compressed result.
pub fn build_context_compression_completed_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    target_context_id: &str,
    array_version: u64,
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
        (KEY_ARRAY_VERSION, serde_json::json!(array_version)),
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

/// Build a LLM_STREAM_ERROR event.
///
/// Emitted by stream consumers (workflow LLM node, agent loop) when a
/// streaming call terminates with a transport/provider error. Carries the
/// error message and the profile id for correlation.
pub fn build_llm_stream_error_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    error: &str,
    profile_id: &str,
) -> BaseEvent {
    let mut event = base_event(EventType::LlmStreamError, execution_id, agent_loop_id);
    event.metadata = Some(metadata(vec![
        (KEY_STREAM_ERROR, serde_json::json!(error)),
        (KEY_PROFILE_ID, serde_json::json!(profile_id)),
    ]));
    event
}

/// Build a LLM_STREAM_ABORTED event.
///
/// Emitted when a streaming call is aborted rather than failing: client
/// cancellation or the dead-loop detector stopping the stream. Carries the
/// abort reason and the profile id for correlation.
pub fn build_llm_stream_aborted_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    reason: &str,
    profile_id: &str,
) -> BaseEvent {
    let mut event = base_event(EventType::LlmStreamAborted, execution_id, agent_loop_id);
    event.metadata = Some(metadata(vec![
        (KEY_STREAM_ABORT_REASON, serde_json::json!(reason)),
        (KEY_PROFILE_ID, serde_json::json!(profile_id)),
    ]));
    event
}

/// Classify a stream termination error: client cancellation (`Cancelled`)
/// is an abort, everything else is an error. Consumers use this to pick
/// between the two event builders.
pub fn is_stream_abort(error: &LlmError) -> bool {
    matches!(error, LlmError::Cancelled)
}

/// Error returned when an event cannot be parsed into a typed metadata
/// struct: wrong event type, missing metadata, or a key with an invalid
/// value. Consumers surface this instead of silently degrading.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TokenEventMetaError {
    #[error("event type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    #[error("event carries no metadata")]
    NoMetadata,
    #[error("event metadata missing required key: {0}")]
    MissingKey(&'static str),
    #[error("event metadata key {0} has invalid value: {1}")]
    InvalidValue(&'static str, String),
}

/// Typed metadata of a [`EventType::TokenUsageWarning`] event.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenUsageWarningMeta {
    pub tokens_used: u64,
    pub token_limit: u64,
    pub usage_percentage: f64,
}

/// Typed metadata of a [`EventType::TokenLimitExceeded`] event.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenLimitExceededMeta {
    pub tokens_used: u64,
    pub token_limit: u64,
}

/// Typed metadata of a [`EventType::ContextCompressionRequested`] event.
///
/// `messages` is the snapshot of the target message array carried by the
/// event; it is empty when the emitting execution did not attach one.
/// `forced` marks the safety-net path (API context-length error).
#[derive(Debug, Clone, PartialEq)]
pub struct ContextCompressionRequestedMeta {
    pub target_context_id: String,
    pub tokens_used: u64,
    pub token_limit: u64,
    pub message_count: usize,
    pub array_version: u64,
    pub forced: bool,
    pub messages: Vec<Message>,
}

/// Typed metadata of a [`EventType::ContextCompressionCompleted`] event.
///
/// `messages` is the compressed message array; `summary` is the optional
/// summary text extracted from it. `array_version` is the version of the
/// target array the compression was produced from: a consumer applies the
/// compressed array only when its own array is still at that version.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextCompressionCompletedMeta {
    pub target_context_id: String,
    pub array_version: u64,
    pub summary: Option<String>,
    pub tokens_after: u64,
    pub messages: Vec<Message>,
}

fn expect_type(event: &BaseEvent, expected: EventType) -> Result<(), TokenEventMetaError> {
    if event.r#type == expected {
        Ok(())
    } else {
        Err(TokenEventMetaError::TypeMismatch {
            expected: expected.as_str(),
            actual: event.r#type.as_str(),
        })
    }
}

fn event_metadata(event: &BaseEvent) -> Result<&wf_types::Metadata, TokenEventMetaError> {
    event
        .metadata
        .as_ref()
        .ok_or(TokenEventMetaError::NoMetadata)
}

fn get_u64(meta: &wf_types::Metadata, key: &'static str) -> Result<u64, TokenEventMetaError> {
    let value = meta.get(key).ok_or(TokenEventMetaError::MissingKey(key))?;
    value
        .as_u64()
        .ok_or_else(|| TokenEventMetaError::InvalidValue(key, value.to_string()))
}

fn get_f64(meta: &wf_types::Metadata, key: &'static str) -> Result<f64, TokenEventMetaError> {
    let value = meta.get(key).ok_or(TokenEventMetaError::MissingKey(key))?;
    value
        .as_f64()
        .ok_or_else(|| TokenEventMetaError::InvalidValue(key, value.to_string()))
}

fn get_string(meta: &wf_types::Metadata, key: &'static str) -> Result<String, TokenEventMetaError> {
    let value = meta.get(key).ok_or(TokenEventMetaError::MissingKey(key))?;
    value
        .as_str()
        .map(String::from)
        .ok_or_else(|| TokenEventMetaError::InvalidValue(key, value.to_string()))
}

/// Messages embedded on an event under [`KEY_MESSAGES`]; absent or malformed
/// snapshots degrade to an empty array (the event type itself is still
/// validated by the `TryFrom` impls).
fn get_messages(meta: &wf_types::Metadata) -> Vec<Message> {
    meta.get(KEY_MESSAGES)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

impl TryFrom<&BaseEvent> for TokenUsageWarningMeta {
    type Error = TokenEventMetaError;

    fn try_from(event: &BaseEvent) -> Result<Self, Self::Error> {
        expect_type(event, EventType::TokenUsageWarning)?;
        let meta = event_metadata(event)?;
        Ok(Self {
            tokens_used: get_u64(meta, KEY_TOKENS_USED)?,
            token_limit: get_u64(meta, KEY_TOKEN_LIMIT)?,
            usage_percentage: get_f64(meta, KEY_USAGE_PERCENTAGE)?,
        })
    }
}

impl TryFrom<&BaseEvent> for TokenLimitExceededMeta {
    type Error = TokenEventMetaError;

    fn try_from(event: &BaseEvent) -> Result<Self, Self::Error> {
        expect_type(event, EventType::TokenLimitExceeded)?;
        let meta = event_metadata(event)?;
        Ok(Self {
            tokens_used: get_u64(meta, KEY_TOKENS_USED)?,
            token_limit: get_u64(meta, KEY_TOKEN_LIMIT)?,
        })
    }
}

impl TryFrom<&BaseEvent> for ContextCompressionRequestedMeta {
    type Error = TokenEventMetaError;

    fn try_from(event: &BaseEvent) -> Result<Self, Self::Error> {
        expect_type(event, EventType::ContextCompressionRequested)?;
        let meta = event_metadata(event)?;
        Ok(Self {
            target_context_id: get_string(meta, KEY_TARGET_CONTEXT_ID)?,
            tokens_used: get_u64(meta, KEY_TOKENS_USED)?,
            token_limit: get_u64(meta, KEY_TOKEN_LIMIT)?,
            message_count: get_u64(meta, KEY_MESSAGE_COUNT)? as usize,
            array_version: meta
                .get(KEY_ARRAY_VERSION)
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            forced: meta
                .get(KEY_FORCED)
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            messages: get_messages(meta),
        })
    }
}

impl TryFrom<&BaseEvent> for ContextCompressionCompletedMeta {
    type Error = TokenEventMetaError;

    fn try_from(event: &BaseEvent) -> Result<Self, Self::Error> {
        expect_type(event, EventType::ContextCompressionCompleted)?;
        let meta = event_metadata(event)?;
        Ok(Self {
            target_context_id: get_string(meta, KEY_TARGET_CONTEXT_ID)?,
            array_version: get_u64(meta, KEY_ARRAY_VERSION)?,
            summary: meta
                .get(KEY_SUMMARY)
                .and_then(|value| value.as_str().map(String::from)),
            tokens_after: get_u64(meta, KEY_TOKENS_AFTER)?,
            messages: get_messages(meta),
        })
    }
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
        let event = build_context_compression_requested_event(
            "exec-1", None, "chat", 1200, 1000, 42, 7, false, None,
        );
        assert_eq!(event.r#type, EventType::ContextCompressionRequested);
        let meta = event.metadata.unwrap();
        assert_eq!(meta[KEY_TARGET_CONTEXT_ID], serde_json::json!("chat"));
        assert_eq!(meta[KEY_MESSAGE_COUNT], serde_json::json!(42));
        assert_eq!(meta[KEY_ARRAY_VERSION], serde_json::json!(7));
        assert!(!meta.contains_key(KEY_FORCED));
        assert!(!meta.contains_key(KEY_MESSAGES));

        // Forced emission is marked.
        let forced = build_context_compression_requested_event(
            "exec-1", None, "chat", 1200, 1000, 42, 7, true, None,
        );
        let forced_meta = forced.metadata.unwrap();
        assert_eq!(forced_meta[KEY_FORCED], serde_json::json!(true));

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
            1,
            false,
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
            42,
            Some("summary"),
            300,
            Some(std::slice::from_ref(&msg)),
        );
        assert_eq!(event.r#type, EventType::ContextCompressionCompleted);
        let meta = event.metadata.unwrap();
        assert_eq!(meta[KEY_TARGET_CONTEXT_ID], serde_json::json!("chat"));
        assert_eq!(meta[KEY_ARRAY_VERSION], serde_json::json!(42));
        assert_eq!(meta[KEY_SUMMARY], serde_json::json!("summary"));
        assert_eq!(meta[KEY_TOKENS_AFTER], serde_json::json!(300));
        let messages: Vec<wf_types::message::Message> =
            serde_json::from_value(meta[KEY_MESSAGES].clone()).unwrap();
        assert_eq!(messages, vec![msg]);

        let no_summary =
            build_context_compression_completed_event("exec-1", None, "chat", 0, None, 0, None);
        let meta = no_summary.metadata.unwrap();
        assert!(!meta.contains_key(KEY_SUMMARY));
        assert!(!meta.contains_key(KEY_MESSAGES));
    }

    #[test]
    fn test_stream_error_event() {
        let event = build_llm_stream_error_event("exec-1", Some("node-1"), "HTTP 500", "p1");
        assert_eq!(event.r#type, EventType::LlmStreamError);
        assert_eq!(event.execution_id.as_deref(), Some("exec-1"));
        assert_eq!(event.agent_loop_id.as_deref(), Some("node-1"));
        let meta = event.metadata.unwrap();
        assert_eq!(meta[KEY_STREAM_ERROR], serde_json::json!("HTTP 500"));
        assert_eq!(meta[KEY_PROFILE_ID], serde_json::json!("p1"));
    }

    #[test]
    fn test_stream_aborted_event() {
        let event = build_llm_stream_aborted_event("exec-1", None, "cancelled", "p1");
        assert_eq!(event.r#type, EventType::LlmStreamAborted);
        let meta = event.metadata.unwrap();
        assert_eq!(
            meta[KEY_STREAM_ABORT_REASON],
            serde_json::json!("cancelled")
        );
        assert_eq!(meta[KEY_PROFILE_ID], serde_json::json!("p1"));
    }

    #[test]
    fn test_stream_abort_classification() {
        assert!(is_stream_abort(&LlmError::Cancelled));
        assert!(!is_stream_abort(&LlmError::StreamError("boom".to_string())));
        assert!(!is_stream_abort(&LlmError::ConfigError("bad".to_string())));
    }

    #[test]
    fn test_parse_warning_meta_roundtrip() {
        let event = build_token_usage_warning_event("exec-1", Some("loop-1"), 900, 1000, 87.5);
        let meta = TokenUsageWarningMeta::try_from(&event).unwrap();
        assert_eq!(meta.tokens_used, 900);
        assert_eq!(meta.token_limit, 1000);
        assert!((meta.usage_percentage - 87.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_limit_exceeded_meta() {
        let event = build_token_limit_exceeded_event("exec-1", None, 1200, 1000);
        let meta = TokenLimitExceededMeta::try_from(&event).unwrap();
        assert_eq!(meta.tokens_used, 1200);
        assert_eq!(meta.token_limit, 1000);
    }

    #[test]
    fn test_parse_compression_requested_meta() {
        let msg = wf_types::message::Message {
            id: "m1".to_string(),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text("hello".to_string()),
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
            3,
            5,
            true,
            Some(std::slice::from_ref(&msg)),
        );
        let meta = ContextCompressionRequestedMeta::try_from(&event).unwrap();
        assert_eq!(meta.target_context_id, "chat");
        assert_eq!(meta.tokens_used, 1200);
        assert_eq!(meta.token_limit, 1000);
        assert_eq!(meta.message_count, 3);
        assert_eq!(meta.array_version, 5);
        assert!(meta.forced);
        assert_eq!(meta.messages, vec![msg]);

        // Absent snapshot/version/forced degrade gracefully, not an error.
        let bare = build_context_compression_requested_event(
            "exec-1", None, "chat", 1200, 1000, 0, 0, false, None,
        );
        let meta = ContextCompressionRequestedMeta::try_from(&bare).unwrap();
        assert!(meta.messages.is_empty());
        assert_eq!(meta.array_version, 0);
        assert!(!meta.forced);
    }

    #[test]
    fn test_parse_compression_completed_meta() {
        let msg = wf_types::message::Message {
            id: "m1".to_string(),
            role: wf_types::message::MessageRole::Assistant,
            content: wf_types::message::MessageContentValue::Text("summarized".to_string()),
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
            7,
            Some("summarized"),
            12,
            Some(&[msg]),
        );
        let meta = ContextCompressionCompletedMeta::try_from(&event).unwrap();
        assert_eq!(meta.target_context_id, "chat");
        assert_eq!(meta.array_version, 7);
        assert_eq!(meta.summary.as_deref(), Some("summarized"));
        assert_eq!(meta.tokens_after, 12);
        assert_eq!(meta.messages.len(), 1);
    }

    #[test]
    fn test_parse_rejects_wrong_event_type() {
        let event = build_token_usage_warning_event("exec-1", None, 900, 1000, 90.0);
        let err = TokenLimitExceededMeta::try_from(&event).unwrap_err();
        assert_eq!(
            err,
            TokenEventMetaError::TypeMismatch {
                expected: "TOKEN_LIMIT_EXCEEDED",
                actual: "TOKEN_USAGE_WARNING",
            }
        );
    }

    #[test]
    fn test_parse_reports_missing_key() {
        let mut event = build_token_limit_exceeded_event("exec-1", None, 1200, 1000);
        let metadata = event.metadata.as_mut().unwrap();
        metadata.remove(KEY_TOKEN_LIMIT);
        let err = TokenLimitExceededMeta::try_from(&event).unwrap_err();
        assert_eq!(err, TokenEventMetaError::MissingKey(KEY_TOKEN_LIMIT));
    }

    #[test]
    fn test_parse_rejects_event_without_metadata() {
        let event = BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::TokenLimitExceeded,
            timestamp: wf_common::now(),
            workflow_id: None,
            execution_id: None,
            agent_loop_id: None,
            metadata: None,
        };
        let err = TokenLimitExceededMeta::try_from(&event).unwrap_err();
        assert_eq!(err, TokenEventMetaError::NoMetadata);
    }
}
