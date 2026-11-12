use wf_llm::LlmError;
use wf_types::events::{BaseEvent, EventType};

use super::keys::{
    base_event, metadata, KEY_ARRAY_VERSION, KEY_COMPLETION_TOKENS, KEY_MESSAGES,
    KEY_MESSAGE_COUNT, KEY_MODEL, KEY_PROFILE_ID, KEY_PROMPT_TOKENS, KEY_STREAM_ABORT_REASON,
    KEY_STREAM_ERROR, KEY_TARGET_CONTEXT_ID, KEY_TOKENS_USED, KEY_TOKEN_LIMIT, KEY_TOOL_COUNT,
    KEY_USAGE_PERCENTAGE, KEY_WRITEBACK_OPERATION,
};

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

/// Build a CONVERSATION_WRITEBACK_COMPLETED event.
///
/// Published by the triggered (nested) agent write-back path when a child
/// agent result is injected into the parent conversation. Carries the
/// operation (`replace` / `append`), the message array to apply, and the
/// anchor version the write-back was produced from: the conversation
/// consumer applies it only while the session is still at that version
/// (stale results are discarded, mirroring the compression write-back).
pub fn build_conversation_writeback_completed_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    target_context_id: &str,
    array_version: u64,
    operation: &str,
    messages: &[wf_types::message::Message],
) -> BaseEvent {
    let mut event = base_event(
        EventType::ConversationWritebackCompleted,
        execution_id,
        agent_loop_id,
    );
    let mut pairs = vec![
        (KEY_TARGET_CONTEXT_ID, serde_json::json!(target_context_id)),
        (KEY_ARRAY_VERSION, serde_json::json!(array_version)),
        (KEY_WRITEBACK_OPERATION, serde_json::json!(operation)),
    ];
    if let Ok(value) = serde_json::to_value(messages) {
        pairs.push((KEY_MESSAGES, value));
    }
    event.metadata = Some(metadata(pairs));
    event
}
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

/// Build a LLM_REQUESTED event.
///
/// Emitted when an agent iteration issues a (non-streaming) LLM request,
/// before the gateway call begins. Carries profile id, message count and
/// tool count for online visibility.
pub fn build_llm_requested_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    profile_id: &str,
    message_count: usize,
    tool_count: usize,
) -> BaseEvent {
    let mut event = base_event(EventType::LlmRequested, execution_id, agent_loop_id);
    event.metadata = Some(metadata(vec![
        (KEY_PROFILE_ID, serde_json::json!(profile_id)),
        (KEY_MESSAGE_COUNT, serde_json::json!(message_count)),
        (KEY_TOOL_COUNT, serde_json::json!(tool_count)),
    ]));
    event
}

/// Build a LLM_RESPONDED event.
///
/// Emitted when a (non-streaming) LLM request completed. Carries profile id,
/// model and reported token usage.
pub fn build_llm_responded_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    profile_id: &str,
    model: Option<&str>,
    prompt_tokens: u32,
    completion_tokens: u32,
) -> BaseEvent {
    let mut event = base_event(EventType::LlmResponded, execution_id, agent_loop_id);
    let mut pairs = vec![
        (KEY_PROFILE_ID, serde_json::json!(profile_id)),
        (KEY_PROMPT_TOKENS, serde_json::json!(prompt_tokens)),
        (KEY_COMPLETION_TOKENS, serde_json::json!(completion_tokens)),
    ];
    if let Some(model) = model {
        pairs.push((KEY_MODEL, serde_json::json!(model)));
    }
    event.metadata = Some(metadata(pairs));
    event
}

/// Build a LLM_FAILED event.
///
/// Emitted when a (non-streaming) LLM request failed. Carries the error
/// message and the profile id for correlation.
pub fn build_llm_failed_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    error: &str,
    profile_id: &str,
) -> BaseEvent {
    let mut event = base_event(EventType::LlmFailed, execution_id, agent_loop_id);
    event.metadata = Some(metadata(vec![
        (KEY_STREAM_ERROR, serde_json::json!(error)),
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
