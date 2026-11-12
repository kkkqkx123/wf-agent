use wf_types::events::{BaseEvent, EventType};
use wf_types::message::Message;

use super::keys::{
    KEY_ARRAY_VERSION, KEY_BLOCKS, KEY_BUDGET_UNKNOWN, KEY_COMPRESSION_ATTEMPTS,
    KEY_COMPRESSION_ERROR, KEY_COMPRESSION_RUN_ID, KEY_CURRENT_VERSION, KEY_DEGRADED,
    KEY_DEGRADED_DROPPED, KEY_DIRECTIVE, KEY_DISCARD_REASON, KEY_FORCED, KEY_MESSAGES,
    KEY_MESSAGE_COUNT, KEY_NO_TAKER, KEY_PLAN, KEY_STILL_OVER_BUDGET, KEY_TAIL_KEEP,
    KEY_TARGET_CONTEXT_ID, KEY_TOKENS_AFTER, KEY_TOKENS_USED, KEY_TOKEN_LIMIT,
    KEY_USAGE_PERCENTAGE, KEY_WRITEBACK_OPERATION,
};

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
/// The audit copy carries identity and accounting only; the message snapshot
/// travels exclusively on the hook channel. `forced` marks the safety-net
/// path (API context-length error).
#[derive(Debug, Clone, PartialEq)]
pub struct ContextCompressionRequestedMeta {
    pub target_context_id: String,
    pub tokens_used: u64,
    pub token_limit: u64,
    pub message_count: usize,
    pub array_version: u64,
    pub forced: bool,
    pub budget_unknown: bool,
    pub no_taker: bool,
    /// Work plan carried by routed handoffs; empty on audit copies and on
    /// old events predating the key (legacy single-summary mode).
    pub plan: Vec<crate::compaction::CompactionPlanItem>,
    /// Request-level instruction; empty when the suggestion carried none
    /// and on old events predating the key.
    pub directive: String,
}

/// Typed metadata of a [`EventType::ContextCompressionCompleted`] event.
///
/// `messages` is the compressed message array (present only when the
/// completed event is the write-back transport, i.e. agent targets;
/// workflow targets write back through the registry instead).
/// `array_version` is the version of the target array the compression was
/// produced from: a consumer applies the compressed array only when its
/// own array is still at that version. `tail_keep` is how many recent
/// pre-existing messages stay visible alongside the summary.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextCompressionCompletedMeta {
    pub target_context_id: String,
    pub array_version: u64,
    pub tokens_after: u64,
    pub messages: Vec<Message>,
    pub tail_keep: usize,
    /// Terminal-failure fallback marker (a locally trimmed window landed).
    pub degraded: bool,
    /// Messages dropped without a summary on a degraded fallback.
    pub degraded_dropped: u64,
    /// The compressed result is still over the emission budget.
    pub still_over_budget: bool,
    /// Run identity stamped by the compression service (`None` on old
    /// events predating the key).
    pub run_id: Option<String>,
    /// Per-block outcomes for plan-mode completions; empty on legacy
    /// single-summary completions and old events predating the key.
    pub blocks: Vec<wf_types::message::BlockOutcome>,
}

/// Typed metadata of a [`EventType::ContextCompressionFailed`] event.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextCompressionFailedMeta {
    pub target_context_id: String,
    pub array_version: u64,
    pub attempts: u32,
    pub error: String,
    /// Run identity stamped by the compression service (`None` on old events).
    pub run_id: Option<String>,
}

/// Typed metadata of a [`EventType::ContextCompressionDiscarded`] event.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextCompressionDiscardedMeta {
    pub target_context_id: String,
    pub array_version: u64,
    pub current_version: u64,
    pub reason: String,
    /// Run identity of the discarded run (`None` on old events).
    pub run_id: Option<String>,
}

/// Typed metadata of a [`EventType::ConversationWritebackCompleted`] event.
///
/// `operation` is always `append` (history is append-only; the `replace`
/// wire name is rejected); `messages` is the message array to apply.
/// `array_version` is the anchor version the write-back was produced
/// from: the conversation consumer applies it only when the session is still
/// at that version.
#[derive(Debug, Clone, PartialEq)]
pub struct ConversationWritebackCompletedMeta {
    pub target_context_id: String,
    pub array_version: u64,
    pub operation: String,
    pub messages: Vec<Message>,
}

/// Optional run identity of a typed compression metadata struct (`None`
/// when the event predates the key).
fn get_optional_run_id(meta: &wf_types::Metadata) -> Option<String> {
    meta.get(KEY_COMPRESSION_RUN_ID)
        .and_then(|value| value.as_str())
        .map(String::from)
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

fn get_bool(meta: &wf_types::Metadata, key: &'static str) -> Result<bool, TokenEventMetaError> {
    let value = meta.get(key).ok_or(TokenEventMetaError::MissingKey(key))?;
    value
        .as_bool()
        .ok_or_else(|| TokenEventMetaError::InvalidValue(key, value.to_string()))
}

/// Messages embedded on an event under [`KEY_MESSAGES`]; absent or malformed
/// snapshots degrade to an empty array (the event type itself is still
/// validated by the `TryFrom` impls).
pub(super) fn get_messages(meta: &wf_types::Metadata) -> Vec<Message> {
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
            array_version: get_u64(meta, KEY_ARRAY_VERSION)?,
            forced: get_bool(meta, KEY_FORCED)?,
            budget_unknown: get_bool(meta, KEY_BUDGET_UNKNOWN)?,
            no_taker: get_bool(meta, KEY_NO_TAKER)?,
            plan: meta
                .get(KEY_PLAN)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            directive: meta
                .get(KEY_DIRECTIVE)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
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
            tokens_after: get_u64(meta, KEY_TOKENS_AFTER)?,
            messages: get_messages(meta),
            tail_keep: get_u64(meta, KEY_TAIL_KEEP)? as usize,
            degraded: get_bool(meta, KEY_DEGRADED)?,
            degraded_dropped: meta
                .get(KEY_DEGRADED_DROPPED)
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            still_over_budget: get_bool(meta, KEY_STILL_OVER_BUDGET)?,
            run_id: get_optional_run_id(meta),
            blocks: meta
                .get(KEY_BLOCKS)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
        })
    }
}

impl TryFrom<&BaseEvent> for ContextCompressionFailedMeta {
    type Error = TokenEventMetaError;

    fn try_from(event: &BaseEvent) -> Result<Self, Self::Error> {
        expect_type(event, EventType::ContextCompressionFailed)?;
        let meta = event_metadata(event)?;
        Ok(Self {
            target_context_id: get_string(meta, KEY_TARGET_CONTEXT_ID)?,
            array_version: get_u64(meta, KEY_ARRAY_VERSION)?,
            attempts: get_u64(meta, KEY_COMPRESSION_ATTEMPTS)? as u32,
            error: get_string(meta, KEY_COMPRESSION_ERROR)?,
            run_id: get_optional_run_id(meta),
        })
    }
}

impl TryFrom<&BaseEvent> for ContextCompressionDiscardedMeta {
    type Error = TokenEventMetaError;

    fn try_from(event: &BaseEvent) -> Result<Self, Self::Error> {
        expect_type(event, EventType::ContextCompressionDiscarded)?;
        let meta = event_metadata(event)?;
        Ok(Self {
            target_context_id: get_string(meta, KEY_TARGET_CONTEXT_ID)?,
            array_version: get_u64(meta, KEY_ARRAY_VERSION)?,
            current_version: get_u64(meta, KEY_CURRENT_VERSION)?,
            reason: get_string(meta, KEY_DISCARD_REASON)?,
            run_id: get_optional_run_id(meta),
        })
    }
}

impl TryFrom<&BaseEvent> for ConversationWritebackCompletedMeta {
    type Error = TokenEventMetaError;

    fn try_from(event: &BaseEvent) -> Result<Self, Self::Error> {
        expect_type(event, EventType::ConversationWritebackCompleted)?;
        let meta = event_metadata(event)?;
        Ok(Self {
            target_context_id: get_string(meta, KEY_TARGET_CONTEXT_ID)?,
            array_version: get_u64(meta, KEY_ARRAY_VERSION)?,
            operation: get_string(meta, KEY_WRITEBACK_OPERATION)?,
            messages: get_messages(meta),
        })
    }
}
