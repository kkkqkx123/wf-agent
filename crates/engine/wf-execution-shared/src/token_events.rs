//! Token and stream event builders
//!
//! Constructs `BaseEvent`s for token usage warnings, limit exceedance,
//! context compression, and LLM stream termination (error / abort). Metadata
//! keys follow a fixed schema (see constants below) so trigger conditions and
//! external consumers can match on them.

pub mod compression;
pub mod keys;
pub mod meta;
pub mod usage;

#[cfg(test)]
mod tests;

pub use compression::{
    build_context_compression_completed_event, build_context_compression_discarded_event,
    build_context_compression_failed_event, build_context_compression_requested_event,
    build_context_compression_routed_event, compression_request_hook_data,
    compression_routed_messages, compression_routed_plan, compression_run_id,
    is_compression_routed_event, match_compression_failure, set_compression_run_id,
    CompressionAttachment, CompressionFailureMatch, ContextCompressionCompleted,
    ContextCompressionRequest,
};
pub use keys::{
    COMPRESSION_SETTLE_POLL_MS, COMPRESSION_SIGNAL_HOOK_TYPE, COMPRESSION_TAKEOVER_HANDLER_NAME,
    DEFAULT_COMPRESSION_SETTLE_TIMEOUT_MS, DEFAULT_COMPRESSION_TAIL_KEEP,
    DEFAULT_TOKEN_WARNING_THRESHOLD, KEY_ARRAY_VERSION, KEY_BLOCKS, KEY_BUDGET_UNKNOWN,
    KEY_CANCELLED, KEY_COMPLETION_TOKENS, KEY_COMPRESSION_ATTEMPTS, KEY_COMPRESSION_DEPTH,
    KEY_COMPRESSION_ERROR, KEY_COMPRESSION_ROUTED, KEY_COMPRESSION_RUN_ID, KEY_CURRENT_VERSION,
    KEY_DEGRADED, KEY_DEGRADED_DROPPED, KEY_DIRECTIVE, KEY_DISCARD_REASON, KEY_FORCED,
    KEY_INJECTED_MESSAGE_COUNT, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_MODEL, KEY_NO_TAKER, KEY_PLAN,
    KEY_PLAN_COUNT, KEY_PROFILE_ID, KEY_PROMPT_TOKENS, KEY_STILL_OVER_BUDGET,
    KEY_STREAM_ABORT_REASON, KEY_STREAM_ERROR, KEY_TAIL_KEEP, KEY_TARGET_CONTEXT_ID,
    KEY_TOKENS_AFTER, KEY_TOKENS_USED, KEY_TOKEN_LIMIT, KEY_TOOL_COUNT, KEY_USAGE_PERCENTAGE,
    KEY_WRITEBACK_OPERATION, WRITEBACK_OPERATION_APPEND,
};
pub use meta::{
    ContextCompressionCompletedMeta, ContextCompressionDiscardedMeta, ContextCompressionFailedMeta,
    ContextCompressionRequestedMeta, ConversationWritebackCompletedMeta, TokenEventMetaError,
    TokenLimitExceededMeta, TokenUsageWarningMeta,
};
pub use usage::{
    build_conversation_writeback_completed_event, build_llm_failed_event,
    build_llm_requested_event, build_llm_responded_event, build_llm_stream_aborted_event,
    build_llm_stream_error_event, build_token_limit_exceeded_event,
    build_token_usage_warning_event, is_stream_abort,
};

/// Process-wide settle budget, seeded from the runtime execution config.
/// Written exactly once during bootstrap (before any execution can run) and
/// read-only afterwards, so the workflow and agent settle waits stay
/// identical by construction.
static COMPRESSION_SETTLE_TIMEOUT_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(DEFAULT_COMPRESSION_SETTLE_TIMEOUT_MS);

/// Inject the configured settle timeout (milliseconds). Non-zero values only;
/// a zero or absent configuration keeps the built-in default. Returns true
/// when the value was stored and false when the input was ignored.
pub fn set_compression_settle_timeout_ms(timeout_ms: u64) -> bool {
    if timeout_ms == 0 {
        return false;
    }
    COMPRESSION_SETTLE_TIMEOUT_MS.store(timeout_ms, std::sync::atomic::Ordering::Relaxed);
    true
}

/// Effective settle budget for compression waits.
pub fn compression_settle_timeout_ms() -> u64 {
    COMPRESSION_SETTLE_TIMEOUT_MS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Resolve the effective settle budget: an explicit injection wins, but an
/// assembly-time correction that only bumps the global still lifts holders
/// of a stale explicit value up to the worst-case claim cost. None reads
/// the process-wide install directly so global fallback stays explicit at
/// the call site.
pub fn effective_settle_timeout_ms(explicit: Option<u64>) -> u64 {
    let global = compression_settle_timeout_ms();
    explicit.map(|value| value.max(global)).unwrap_or(global)
}
