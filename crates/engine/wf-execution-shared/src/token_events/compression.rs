use std::sync::Arc;

use wf_types::events::{BaseEvent, EventType};
use wf_types::message::Message;

use super::keys::{
    base_event, metadata, KEY_ARRAY_VERSION, KEY_BLOCKS, KEY_BUDGET_UNKNOWN,
    KEY_COMPRESSION_ATTEMPTS, KEY_COMPRESSION_DEPTH, KEY_COMPRESSION_ERROR, KEY_COMPRESSION_ROUTED,
    KEY_COMPRESSION_RUN_ID, KEY_CURRENT_VERSION, KEY_DEGRADED, KEY_DIRECTIVE, KEY_DISCARD_REASON,
    KEY_FORCED, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_NO_TAKER, KEY_PLAN, KEY_PLAN_COUNT,
    KEY_STILL_OVER_BUDGET, KEY_TAIL_KEEP, KEY_TARGET_CONTEXT_ID, KEY_TOKENS_AFTER, KEY_TOKENS_USED,
    KEY_TOKEN_LIMIT,
};
use super::meta::{get_messages, ContextCompressionFailedMeta};

/// Process-local compression payload carried on a hook context alongside
/// the JSON audit fields. The synchronous handler takes over from
/// the attachment and skips the snapshot/plan JSON round-trip entirely;
/// the routed handoff serializes from it once at publish time.
#[derive(Debug, Clone)]
pub struct CompressionAttachment {
    pub messages: Arc<Vec<Message>>,
    pub plan: Arc<Vec<crate::compaction::CompactionPlanItem>>,
}

/// One `CONTEXT_COMPRESSION_REQUESTED` emission's full data set: the target
/// message array identity, the token accounting that triggered it and the
/// message snapshot. The snapshot travels only on the functional channel
/// (the hook dispatch, whose takeover needs it); the audit event copy
/// carries just the identity and accounting fields.
pub struct ContextCompressionRequest<'a> {
    /// Name of the message array targeted by the compression.
    pub target_context_id: &'a str,
    /// Estimated tokens of the target array at emission time.
    pub tokens_used: u64,
    /// Token budget the array is checked against.
    pub token_limit: u64,
    /// Message count of the target array at emission time.
    pub message_count: usize,
    /// Ledger version of the target array at emission time (idempotency).
    pub array_version: u64,
    /// `true` when the request is a safety-net re-emission over real request
    /// messages after an API context-length-exceeded error.
    pub forced: bool,
    /// Snapshot of the messages the compression will summarize.
    pub messages: &'a [Message],
    /// Compaction work plan over the snapshot. Empty means the legacy
    /// single-summary mode; a non-empty plan makes the service run the
    /// summary chain once per item and write back one aggregated result.
    pub plan: &'a [crate::compaction::CompactionPlanItem],
    /// Request-level instruction from the planning suggestion, prepended
    /// as a system message to every slice. Empty when the suggestion
    /// carried none; the forced safety-net path always leaves it empty.
    pub directive: &'a str,
}

/// Convert a [`ContextCompressionRequest`] into the hook payload map shared
/// by the agent and workflow compression dispatchers. The map carries the
/// audit vocabulary only (identity, accounting, plan length); the snapshot
/// and plan travel in the hook context attachment (see
/// [`CompressionAttachment`]) so the synchronous handler skips the JSON
/// round-trip. Routed copies serialize from the attachment at publish time.
pub fn compression_request_hook_data(
    request: &ContextCompressionRequest<'_>,
) -> std::collections::HashMap<String, serde_json::Value> {
    let mut data = std::collections::HashMap::new();
    data.insert(
        KEY_TARGET_CONTEXT_ID.to_string(),
        serde_json::json!(request.target_context_id),
    );
    data.insert(
        KEY_TOKENS_USED.to_string(),
        serde_json::json!(request.tokens_used),
    );
    data.insert(
        KEY_TOKEN_LIMIT.to_string(),
        serde_json::json!(request.token_limit),
    );
    data.insert(
        KEY_MESSAGE_COUNT.to_string(),
        serde_json::json!(request.message_count),
    );
    data.insert(
        KEY_ARRAY_VERSION.to_string(),
        serde_json::json!(request.array_version),
    );
    data.insert(KEY_FORCED.to_string(), serde_json::json!(request.forced));
    data.insert(
        KEY_BUDGET_UNKNOWN.to_string(),
        serde_json::json!(request.forced && request.token_limit == 0),
    );
    data.insert(
        KEY_PLAN_COUNT.to_string(),
        serde_json::json!(request.plan.len()),
    );
    data.insert(
        KEY_DIRECTIVE.to_string(),
        serde_json::json!(request.directive),
    );
    data
}

/// Build a CONTEXT_COMPRESSION_REQUESTED event.
///
/// Emitted when a named message array exceeds the configured token limit (or
/// when an API context-length-exceeded error forces it, see `forced`); the
/// engine dispatches the same signal synchronously through the hook registry
/// (the compression service takes over immediately). This event is the audit
/// copy: it carries the target array name (`target_context_id`), its version
/// at emission time (`array_version`, decision-track idempotency), its
/// message count and the token accounting — never the message snapshot,
/// which travels only on the hook channel.
///
/// `forced` distinguishes the two emission reasons: false (default) means
/// the estimated array budget was exceeded (decision track); true means the
/// provider rejected the actual request with a context-length error and the
/// event is a safety-net re-emission over the real request messages.
pub fn build_context_compression_requested_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    request: &ContextCompressionRequest<'_>,
) -> BaseEvent {
    let mut event = base_event(
        EventType::ContextCompressionRequested,
        execution_id,
        agent_loop_id,
    );
    let mut pairs = vec![
        (
            KEY_TARGET_CONTEXT_ID,
            serde_json::json!(request.target_context_id),
        ),
        (KEY_TOKENS_USED, serde_json::json!(request.tokens_used)),
        (KEY_TOKEN_LIMIT, serde_json::json!(request.token_limit)),
        (KEY_MESSAGE_COUNT, serde_json::json!(request.message_count)),
        (KEY_ARRAY_VERSION, serde_json::json!(request.array_version)),
    ];
    pairs.push((KEY_FORCED, serde_json::json!(request.forced)));
    pairs.push((
        KEY_BUDGET_UNKNOWN,
        serde_json::json!(request.forced && request.token_limit == 0),
    ));
    pairs.push((KEY_NO_TAKER, serde_json::json!(false)));
    pairs.push((KEY_PLAN_COUNT, serde_json::json!(request.plan.len())));
    pairs.push((KEY_DIRECTIVE, serde_json::json!(request.directive)));
    event.metadata = Some(metadata(pairs));
    event
}

/// Build the routed handoff copy of a `CONTEXT_COMPRESSION_REQUESTED` event.
///
/// Published by the builtin compression adapter (not by the emitter) after
/// the synchronous takeover: the audit fields and accounting match the
/// emitter's audit copy, plus the message snapshot under [`KEY_MESSAGES`],
/// the [`KEY_COMPRESSION_ROUTED`] marker and the nesting depth. Only the
/// builtin compression template matches this copy; the emitter's audit copy
/// carries identity and accounting only and never matches.
///
/// `depth` is the emitting execution's compression nesting depth.
pub fn build_context_compression_routed_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    request: &ContextCompressionRequest<'_>,
    depth: u64,
) -> BaseEvent {
    let mut event = build_context_compression_requested_event(execution_id, agent_loop_id, request);
    let meta = event.metadata.get_or_insert_with(Default::default);
    if let Ok(value) = serde_json::to_value(request.messages) {
        meta.insert(KEY_MESSAGES.to_string(), value);
    }
    if !request.plan.is_empty() {
        if let Ok(value) = serde_json::to_value(request.plan) {
            meta.insert(KEY_PLAN.to_string(), value);
        }
    }
    meta.insert(KEY_COMPRESSION_ROUTED.to_string(), serde_json::json!(true));
    meta.insert(KEY_COMPRESSION_DEPTH.to_string(), serde_json::json!(depth));
    event
}

/// Whether an event is the adapter-published routed handoff (as opposed to
/// the emitter's audit copy).
pub fn is_compression_routed_event(event: &BaseEvent) -> bool {
    event.r#type == EventType::ContextCompressionRequested
        && event
            .metadata
            .as_ref()
            .and_then(|meta| meta.get(KEY_COMPRESSION_ROUTED))
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
}

/// Messages embedded on a routed compression event under [`KEY_MESSAGES`];
/// absent or malformed snapshots degrade to an empty array (the caller
/// skips snapshot-less handoffs, mirroring the adapter's empty-snapshot skip).
pub fn compression_routed_messages(event: &BaseEvent) -> Vec<Message> {
    event
        .metadata
        .as_ref()
        .map(get_messages)
        .unwrap_or_default()
}

/// Work plan embedded on a routed compression event under [`KEY_PLAN`];
/// absent or malformed plans degrade to an empty plan (legacy
/// single-summary mode) rather than failing the handoff.
pub fn compression_routed_plan(event: &BaseEvent) -> Vec<crate::compaction::CompactionPlanItem> {
    event
        .metadata
        .as_ref()
        .and_then(|meta| meta.get(KEY_PLAN))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

/// Payload of a `CONTEXT_COMPRESSION_COMPLETED` event (everything except the
/// execution identity that `base_event` already carries).
pub struct ContextCompressionCompleted<'a> {
    /// Target array name (workflow named context or agent conversation id).
    pub target_context_id: &'a str,
    /// Array version the compression was produced from.
    pub array_version: u64,
    /// Estimated token count after compression.
    pub tokens_after: u64,
    /// Compressed message array for versioned write-back. Carried only
    /// when the completed event is the write-back transport (agent
    /// targets); workflow targets write back through the registry and
    /// the consumer there reads outcomes plus the anchor only.
    pub messages: Option<&'a [wf_types::message::Message]>,
    /// Recent pre-existing messages kept visible beside the summary.
    pub tail_keep: usize,
    /// True when the result came from the terminal-failure fallback (a locally
    /// trimmed window, no LLM summary) rather than a real summary.
    pub degraded: bool,
    /// True when the compressed result is still over the emission budget.
    pub still_over_budget: bool,
    /// Per-block outcomes for plan-mode completions; empty on legacy
    /// single-summary completions.
    pub blocks: &'a [wf_types::message::BlockOutcome],
}

/// Build a CONTEXT_COMPRESSION_COMPLETED event.
///
/// Carries the target array name, the per-block outcomes and the array
/// version the compression was produced from (the version of the matching
/// REQUESTED event), so any consumer (the main workflow execution, an agent
/// loop) can reproduce the write-back and version-check it without extra
/// lookups: an array that moved past `array_version` must discard the
/// compressed result. The compressed message array travels only when the
/// event is the write-back transport (agent targets).
pub fn build_context_compression_completed_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    payload: &ContextCompressionCompleted<'_>,
) -> BaseEvent {
    let mut event = base_event(
        EventType::ContextCompressionCompleted,
        execution_id,
        agent_loop_id,
    );
    let mut pairs = vec![
        (
            KEY_TARGET_CONTEXT_ID,
            serde_json::json!(payload.target_context_id),
        ),
        (KEY_ARRAY_VERSION, serde_json::json!(payload.array_version)),
        (KEY_TOKENS_AFTER, serde_json::json!(payload.tokens_after)),
        (KEY_TAIL_KEEP, serde_json::json!(payload.tail_keep as u64)),
    ];
    pairs.push((KEY_DEGRADED, serde_json::json!(payload.degraded)));
    pairs.push((
        KEY_STILL_OVER_BUDGET,
        serde_json::json!(payload.still_over_budget),
    ));
    if let Some(messages) = payload.messages {
        if let Ok(value) = serde_json::to_value(messages) {
            pairs.push((KEY_MESSAGES, value));
        }
    }
    if !payload.blocks.is_empty() {
        if let Ok(value) = serde_json::to_value(payload.blocks) {
            pairs.push((KEY_BLOCKS, value));
        }
    }
    event.metadata = Some(metadata(pairs));
    event
}

/// Build a CONTEXT_COMPRESSION_FAILED event.
///
/// Published when a compression run reaches its terminal failure state
/// (retries exhausted or a non-retryable error). Consumers clear their
/// in-flight compression record on this event; the emission guard stays so
/// the same array version does not loop.
pub fn build_context_compression_failed_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    target_context_id: &str,
    array_version: u64,
    attempts: u32,
    error: &str,
) -> BaseEvent {
    let mut event = base_event(
        EventType::ContextCompressionFailed,
        execution_id,
        agent_loop_id,
    );
    event.metadata = Some(metadata(vec![
        (KEY_TARGET_CONTEXT_ID, serde_json::json!(target_context_id)),
        (KEY_ARRAY_VERSION, serde_json::json!(array_version)),
        (KEY_COMPRESSION_ATTEMPTS, serde_json::json!(attempts)),
        (KEY_COMPRESSION_ERROR, serde_json::json!(error)),
    ]));
    event
}

/// Build a CONTEXT_COMPRESSION_DISCARDED event.
///
/// Published when a compression result arrives after its target array moved
/// past the emission version: the stale result is dropped, the anchor is
/// released and the emitter continues. Normal concurrency, never a failure
/// and never a park trigger.
pub fn build_context_compression_discarded_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    target_context_id: &str,
    array_version: u64,
    current_version: u64,
    reason: &str,
) -> BaseEvent {
    let mut event = base_event(
        EventType::ContextCompressionDiscarded,
        execution_id,
        agent_loop_id,
    );
    event.metadata = Some(metadata(vec![
        (KEY_TARGET_CONTEXT_ID, serde_json::json!(target_context_id)),
        (KEY_ARRAY_VERSION, serde_json::json!(array_version)),
        (KEY_CURRENT_VERSION, serde_json::json!(current_version)),
        (KEY_DISCARD_REASON, serde_json::json!(reason)),
    ]));
    event
}

/// Stamp a compression run identity onto a terminal compression event
/// (completed, failed or discarded). Best-effort correlation only: old
/// consumers ignore the unknown key.
pub fn set_compression_run_id(event: &mut BaseEvent, run_id: &str) {
    let meta = event.metadata.get_or_insert_with(Default::default);
    meta.insert(
        KEY_COMPRESSION_RUN_ID.to_string(),
        serde_json::json!(run_id),
    );
}

/// Read the compression run identity stamped by [`set_compression_run_id`]
/// (`None` on old events without the key).
pub fn compression_run_id(event: &BaseEvent) -> Option<String> {
    event
        .metadata
        .as_ref()
        .and_then(|meta| meta.get(KEY_COMPRESSION_RUN_ID))
        .and_then(|value| value.as_str())
        .map(String::from)
}

/// Shared match of a compression failure event against an emitting
/// execution. Single writer for the agent and workflow settle paths so
/// both sides agree on filtering and on the external-handling message.
#[derive(Debug, Clone, PartialEq)]
pub struct CompressionFailureMatch {
    pub target_context_id: String,
    pub array_version: u64,
    pub message: String,
}

/// Match a compression failure event against an emitting execution.
/// Returns the target, anchor version and handling message when the event
/// targets `execution_id` and, when given, `expected_version`.
///
/// The message wording is shared so agent pauses and workflow pauses
/// describe the same failure category in the same terms.
pub fn match_compression_failure(
    event: &BaseEvent,
    execution_id: &str,
    expected_version: Option<u64>,
) -> Option<CompressionFailureMatch> {
    if event.r#type != EventType::ContextCompressionFailed {
        return None;
    }
    if event.execution_id.as_deref() != Some(execution_id) {
        return None;
    }
    let meta = ContextCompressionFailedMeta::try_from(event).ok()?;
    if expected_version.is_some_and(|version| meta.array_version != version) {
        return None;
    }
    let message = format!(
        "context compression failed for '{}' at version {}: {}; the execution pauses for external handling",
        meta.target_context_id, meta.array_version, meta.error
    );
    Some(CompressionFailureMatch {
        target_context_id: meta.target_context_id,
        array_version: meta.array_version,
        message,
    })
}
