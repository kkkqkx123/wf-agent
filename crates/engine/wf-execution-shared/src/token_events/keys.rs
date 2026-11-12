use wf_types::events::{BaseEvent, EventType};

/// Metadata key: tokens used so far.
pub const KEY_TOKENS_USED: &str = "tokens_used";
/// Metadata key: configured token limit.
pub const KEY_TOKEN_LIMIT: &str = "token_limit";
/// Metadata key: percentage of the limit consumed (0-100).
pub const KEY_USAGE_PERCENTAGE: &str = "usage_percentage";
/// Metadata key: conversation message count (compression requested).
pub const KEY_MESSAGE_COUNT: &str = "message_count";
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
/// Metadata key: true when a forced compression request was emitted without
/// a known context budget (no model window). The token limit is reported as
/// zero in that case so consumers never derive a ratio from it.
pub const KEY_BUDGET_UNKNOWN: &str = "budget_unknown";
/// Metadata key: number of recent pre-existing messages the write-back side
/// keeps visible alongside the summary.
pub const KEY_TAIL_KEEP: &str = "tail_keep";
/// Metadata key: true on a `CONTEXT_COMPRESSION_COMPLETED` event produced by
/// the terminal-failure fallback (a locally trimmed window was written back
/// without an LLM summary). Absent/false on a normal summary write-back.
pub const KEY_DEGRADED: &str = "degraded";
/// Metadata key: true on a `CONTEXT_COMPRESSION_COMPLETED` event whose
/// compressed result is still over the emission budget. Lets the emitter
/// re-arm a forced compression instead of waiting for the next append.
pub const KEY_STILL_OVER_BUDGET: &str = "still_over_budget";
/// Hook payload key: compression nesting depth of the emitting execution
/// (absent means zero). The service increments it into the summary input so
/// nested runs refuse their own chain.
pub const KEY_COMPRESSION_DEPTH: &str = "compression_depth";
/// Metadata key: terminal failure reason of a compression run.
pub const KEY_COMPRESSION_ERROR: &str = "error";
/// Metadata key: attempts spent on the failed compression run.
pub const KEY_COMPRESSION_ATTEMPTS: &str = "attempts";
/// Metadata key: true on a `CONTEXT_COMPRESSION_REQUESTED` event whose signal
/// had no hook handler to take over (audit kept, flight never anchored).
pub const KEY_NO_TAKER: &str = "no_taker";

/// Audit marker on the REQUESTED copy when an observer veto cancelled the
/// compression (distinct from `no_taker`: denial, not a missing service).
pub const KEY_CANCELLED: &str = "cancelled";
/// Metadata key: true on a `CONTEXT_COMPRESSION_REQUESTED` event published
/// by the builtin compression adapter as the routed handoff (as opposed to
/// the emitter's audit copy). Only the routed copy carries the message
/// snapshot and only the builtin compression template matches it; user
/// templates can never observe this marker because targeting the signal is
/// rejected at load time.
pub const KEY_COMPRESSION_ROUTED: &str = "compression_routed";
/// Metadata key: number of snapshot messages dropped without a summary on a
/// degraded `CONTEXT_COMPRESSION_COMPLETED` (partial window fallback).
pub const KEY_DEGRADED_DROPPED: &str = "degraded_dropped";
/// Metadata key: reason a `CONTEXT_COMPRESSION_DISCARDED` event discarded its
/// stale result (version mismatch detail).
pub const KEY_DISCARD_REASON: &str = "reason";
/// Metadata key: current array version observed when a stale compression
/// result was discarded (the version that won over the anchor).
pub const KEY_CURRENT_VERSION: &str = "current_version";
/// Metadata key: compression run identity assigned by the compression service
/// at claim time (one id per summary run). Terminal events (completed,
/// failed, discarded) carry it so offline analysis can tell two runs for
/// overlapping versions apart: same `(target, version)` with distinct run
/// ids means a redundant summary whose write-back the version anchor
/// discards. Absent on old events and on the requested audit copy (published
/// by the emitter before the service claims the signal).
pub const KEY_COMPRESSION_RUN_ID: &str = "compression_run_id";
/// Metadata key: number of transform_context-injected messages included in
/// the array budget check (informational; only present when > 0).
pub const KEY_INJECTED_MESSAGE_COUNT: &str = "injected_message_count";
/// Payload key: compaction work plan carried by the compression signal's
/// hook data and the routed handoff. Empty means the legacy single-summary
/// mode, so old emitters stay valid without a plan.
/// Audit copies carry only the plan length under [`KEY_PLAN_COUNT`].
/// Payload key: serialized conversation messages carried by the compression
/// signal's hook data (the functional snapshot for the taking-over service)
/// and by the compression completed event (the compressed array). The audit
/// copy of a requested event never carries it.
pub const KEY_PLAN: &str = "plan";
/// Metadata key: number of compaction work items on an audit copy.
pub const KEY_PLAN_COUNT: &str = "plan_count";
/// Hook data and audit key: request-level instruction from the planning
/// suggestion, carried by every slice of the emission. Empty means the
/// suggestion carried none; the forced safety-net path never carries one.
pub const KEY_DIRECTIVE: &str = "directive";
/// Payload key: serialized conversation messages carried by the compression
pub const KEY_MESSAGES: &str = "messages";
/// Metadata key: per-block outcomes of a plan-mode compression completion.
/// Absent on legacy single-summary completions.
pub const KEY_BLOCKS: &str = "blocks";
/// Metadata key: write-back operation of a `CONVERSATION_WRITEBACK_COMPLETED`
/// event (only `append`; history is append-only).
pub const KEY_WRITEBACK_OPERATION: &str = "operation";
/// Write-back operation value: append to the target array (continuation
/// semantics). This is the only supported write-back operation.
pub const WRITEBACK_OPERATION_APPEND: &str = "append";
/// Metadata key: error message of a failed LLM stream.
pub const KEY_STREAM_ERROR: &str = "error";
/// Metadata key: abort reason of an aborted LLM stream.
pub const KEY_STREAM_ABORT_REASON: &str = "reason";
/// Metadata key: profile id the failing LLM stream was issued against.
pub const KEY_PROFILE_ID: &str = "profile_id";
/// Metadata key: model of a completed LLM call.
pub const KEY_MODEL: &str = "model";
/// Metadata key: prompt tokens reported by a completed LLM call.
pub const KEY_PROMPT_TOKENS: &str = "prompt_tokens";
/// Metadata key: completion tokens reported by a completed LLM call.
pub const KEY_COMPLETION_TOKENS: &str = "completion_tokens";
/// Metadata key: tools offered in an LLM request.
pub const KEY_TOOL_COUNT: &str = "tool_count";

/// Default token warning threshold percentage of the configured limit.
pub const DEFAULT_TOKEN_WARNING_THRESHOLD: u32 = 80;

/// Default number of recent messages kept visible alongside the summary
/// after a compression write-back.
pub const DEFAULT_COMPRESSION_TAIL_KEEP: usize = 2;

/// Poll interval while waiting for an in-flight compression to settle.
pub const COMPRESSION_SETTLE_POLL_MS: u64 = 50;
/// Default budget for the emitting execution's settle wait on an in-flight
/// compression. Emitters pause for external handling on timeout instead of
/// waiting forever when the completion event is lost without a failure
/// event. The runtime replaces this default from
/// `limits.compression.settle_timeout_ms` at bootstrap (see
/// [`set_compression_settle_timeout_ms`]).
pub const DEFAULT_COMPRESSION_SETTLE_TIMEOUT_MS: u64 = 600_000;

/// Hook type of the engine's internal context-compression signal: the engine
/// dispatches it synchronously so registered receivers (the compression
/// service) take over immediately; the `CONTEXT_COMPRESSION_REQUESTED` event
/// remains the audit / persistence / user-rule channel.
pub const COMPRESSION_SIGNAL_HOOK_TYPE: &str = wf_types::hook::CONTEXT_COMPRESSION_SIGNAL;

/// Stable name of the builtin compression takeover: the service registers
/// by name only (never auto-notified by the fire) and the signal dispatch
/// resolves it after the gate check, so an observer veto cancels before any
/// work spawns. Test doubles take over the same way under this name.
pub const COMPRESSION_TAKEOVER_HANDLER_NAME: &str = "context_compression";

pub(super) fn metadata(pairs: Vec<(&str, serde_json::Value)>) -> wf_types::Metadata {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

pub(super) fn base_event(
    event_type: EventType,
    execution_id: &str,
    agent_loop_id: Option<&str>,
) -> BaseEvent {
    BaseEvent {
        id: wf_common::generate_id(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(execution_id.to_string()),
        agent_loop_id: agent_loop_id.map(|s| s.to_string()),
        event_name: None,
        metadata: None,
    }
}
