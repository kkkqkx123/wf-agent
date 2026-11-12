use serde_json::Value;
use wf_core::EventBus;
use wf_llm::LlmGateway;
use wf_types::events::BaseEvent;
use wf_types::llm::LlmRequest;
use wf_types::message::Message;
use wf_types::Id;

use crate::hooks::{fire, HookContext, HookHandlerRegistry};

/// Versioned write-back operation applied to a context array.
///
/// Only append is supported: history is append-only, so a write-back never
/// discards or overwrites existing messages (compression switches the view
/// instead of replacing the array).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritebackOp {
    /// Append messages to the array (continuation semantics).
    Append,
}

impl WritebackOp {
    /// Resolve the wire operation name carried by write-back completed
    /// events (`WRITEBACK_OPERATION_APPEND`). Unknown names are rejected.
    pub fn from_operation_name(operation: &str) -> Option<Self> {
        match operation {
            crate::WRITEBACK_OPERATION_APPEND => Some(Self::Append),
            _ => None,
        }
    }
}

/// Whether the array estimate exceeds its budget (0 disables the check).
/// The check itself never mutates anything; callers emit the compression
/// signal and let the hook-trigger chain decide what happens.
pub fn over_budget(estimated_tokens: u64, token_limit: u64) -> bool {
    token_limit > 0 && estimated_tokens > token_limit
}

/// Build one compression request over a message snapshot. Pure
/// constructor shared by the agent and workflow emitters so both sides
/// agree on the payload shape.
#[allow(clippy::too_many_arguments)]
pub fn compression_request<'a>(
    target_context_id: &'a str,
    tokens_used: u64,
    token_limit: u64,
    message_count: usize,
    array_version: u64,
    forced: bool,
    messages: &'a [Message],
    plan: &'a [crate::CompactionPlanItem],
    directive: &'a str,
) -> crate::ContextCompressionRequest<'a> {
    crate::ContextCompressionRequest {
        target_context_id,
        tokens_used,
        token_limit,
        message_count,
        array_version,
        forced,
        messages,
        plan,
        directive,
    }
}

/// Build the audit copy of a compression request event. Callers publish it
/// through their own bus style, then deliver the signal synchronously via
/// [`dispatch_compression_signal`].
pub fn compression_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    request: &crate::ContextCompressionRequest<'_>,
) -> BaseEvent {
    crate::build_context_compression_requested_event(execution_id, agent_loop_id, request)
}

/// Stamp the taker outcome onto the audit copy of a compression request.
/// Taken leaves the audit unchanged; vetoed marks it cancelled and untaken
/// marks it with the no-taker flag. Pure annotation shared by every
/// emitter so the audit vocabulary never drifts between the agent and
/// workflow paths.
pub fn annotate_compression_audit(
    event: &mut BaseEvent,
    outcome: &CompressionSignalOutcome,
) {
    match outcome {
        CompressionSignalOutcome::Taken => {}
        CompressionSignalOutcome::Vetoed { .. } => {
            if let Some(meta) = event.metadata.as_mut() {
                meta.insert(
                    crate::token_events::KEY_CANCELLED.to_string(),
                    Value::Bool(true),
                );
            }
        }
        CompressionSignalOutcome::Untaken => {
            if let Some(meta) = event.metadata.as_mut() {
                meta.insert(
                    crate::token_events::KEY_NO_TAKER.to_string(),
                    Value::Bool(true),
                );
            }
        }
    }
}

/// Build the pre-request budget warning event with one percentage
/// definition shared by every emitter. Warning only, never blocks the
/// request; the single-shot guard and any array details stay with the
/// caller.
pub fn build_preflight_warning_event(
    execution_id: &str,
    emitter_id: Option<&str>,
    estimated: u64,
    context_limit: u64,
) -> BaseEvent {
    crate::build_token_usage_warning_event(
        execution_id,
        emitter_id,
        estimated,
        context_limit,
        estimated as f64 / context_limit as f64 * 100.0,
    )
}

/// Build the version-mismatch discarded event shared by both write-back
/// paths. The run identity travels along when present so redundant runs
/// stay linkable to their claim.
#[allow(clippy::too_many_arguments)]
pub fn build_stale_discarded_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    target_context_id: &str,
    anchor_version: u64,
    current_version: u64,
    reason: &str,
    run_id: Option<&str>,
) -> BaseEvent {
    let mut discarded = crate::build_context_compression_discarded_event(
        execution_id,
        agent_loop_id,
        target_context_id,
        anchor_version,
        current_version,
        reason,
    );
    if let Some(run_id) = run_id {
        crate::set_compression_run_id(&mut discarded, run_id);
    }
    discarded
}

/// Find the handling message of a terminal compression failure for one
/// anchor version. An optional target restricts the match to a single
/// named array; without it the first failure for the execution wins.
/// Single home so agent and workflow settle closures share the wording
/// and the target filtering.
pub fn find_compression_failure(
    events: &[BaseEvent],
    execution_id: &str,
    version: u64,
    target: Option<&str>,
) -> Option<String> {
    events.iter().find_map(|event| {
        crate::match_compression_failure(event, execution_id, Some(version))
            .filter(|matched| {
                target.is_none_or(|want| matched.target_context_id == want)
            })
            .map(|matched| matched.message.clone())
    })
}

/// Outcome of one compression signal dispatch: the emitter anchors
/// backpressure only on [`CompressionSignalOutcome::Taken`]. A vetoed
/// signal still counts as emitted (the same version must not re-fire every
/// settlement) but clears the pending plan and never anchors a flight; an
/// untaken signal keeps the legacy no-taker path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressionSignalOutcome {
    /// The takeover handler ran (or a supplied replacement was published):
    /// the caller anchors the flight and clears the no-taker streak.
    Taken,
    /// An observer vetoed (or a replacement could not land): the caller
    /// marks the audit copy cancelled, clears the pending plan and records
    /// no no-taker streak. Denial, not a missing service.
    Vetoed { reason: String },
    /// No registry or no named takeover could take over: the caller keeps
    /// the audit event with the no-taker marker and must not anchor.
    Untaken,
}

/// Deliver the compression signal synchronously: observers fire first, then
/// — unless vetoed — the named takeover runs (or a supplied replacement is
/// published). Agent targets attach their loop id so the agent conversation
/// self-consumes the completed event; workflow targets carry no loop id and
/// write back through the execution registry.
///
/// A veto cancels before any work spawns; a replacement skips the summary
/// chain and lands as the terminal completion. Callers anchor backpressure
/// only on [`CompressionSignalOutcome::Taken`].
/// `static_defs` carries the emitting execution's static hook
/// definitions (workflow node definitions; the agent side passes an empty
/// slice while its hook vocabulary has no compression point). The fire
/// filters them by the signal point and reuses condition evaluation,
/// priority merging and handler-name resolution; unresolvable handlers
/// fail open at this gate like everywhere else.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_compression_signal(
    registry: Option<&HookHandlerRegistry>,
    bus: Option<&EventBus>,
    execution_id: &Id,
    agent_loop_id: Option<&Id>,
    cancellation: tokio_util::sync::CancellationToken,
    request: &crate::ContextCompressionRequest<'_>,
    static_defs: &[crate::hooks::HookDefinition],
) -> CompressionSignalOutcome {
    let Some(registry) = registry else {
        tracing::warn!(
            execution_id = %execution_id,
            target = %request.target_context_id,
            version = request.array_version,
            "compression signal dropped: no hook registry to take over"
        );
        return CompressionSignalOutcome::Untaken;
    };
    let mut data = crate::compression_request_hook_data(request);
    if let Some(loop_id) = agent_loop_id {
        data.insert(
            "agent_loop_id".to_string(),
            Value::String(loop_id.to_string()),
        );
    }
    // The snapshot and plan ride the context attachment (process-local
    // shared ownership, never serialized); the hook data map keeps the
    // JSON audit vocabulary only.
    let attachment = crate::CompressionAttachment {
        messages: std::sync::Arc::new(request.messages.to_vec()),
        plan: std::sync::Arc::new(request.plan.to_vec()),
    };
    let ctx = HookContext::new(
        execution_id.clone(),
        crate::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
        data,
        cancellation,
    )
    .with_attachment(attachment);
    let summary = fire(
        registry,
        static_defs,
        crate::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        &ctx,
        bus,
    )
    .await;
    // Gate: an observer veto cancels before the takeover runs. The builtin
    // service registers by name only (never auto-notified), so no work has
    // spawned at this point; the pending plan dies with the cancelled
    // signal and the caller anchors nothing.
    if let Some(reason) = summary.vetoed_reason() {
        tracing::warn!(
            execution_id = %execution_id,
            target = %request.target_context_id,
            version = request.array_version,
            reason = %reason,
            "compression signal vetoed: cancelled before takeover"
        );
        return CompressionSignalOutcome::Vetoed {
            reason: reason.to_string(),
        };
    }
    if let Some(summaries) = summary.replacement {
        return publish_replacement(bus, execution_id, agent_loop_id, request, summaries).await;
    }
    let Some(takeover) = registry.get(crate::token_events::COMPRESSION_TAKEOVER_HANDLER_NAME)
    else {
        tracing::warn!(
            execution_id = %execution_id,
            target = %request.target_context_id,
            version = request.array_version,
            "compression signal dropped: no takeover registered under the well-known name"
        );
        return CompressionSignalOutcome::Untaken;
    };
    registry
        .notify(
            &ctx,
            &crate::hooks::registry::RegisteredHandler {
                name: takeover.name().to_string(),
                priority: 0,
                handler: takeover,
                condition: None,
            },
        )
        .await;
    CompressionSignalOutcome::Taken
}

/// Publish a handler-supplied compression result as the terminal COMPLETED
/// event: the legacy single-summary shape (summary batch plus empty block
/// outcomes), so the version anchor and the consumer apply it unchanged.
/// Without a bus the replacement cannot land: cancel loudly instead of
/// downgrading to the builtin chain behind the supplier's back.
async fn publish_replacement(
    bus: Option<&EventBus>,
    execution_id: &Id,
    agent_loop_id: Option<&Id>,
    request: &crate::ContextCompressionRequest<'_>,
    summaries: Vec<Message>,
) -> CompressionSignalOutcome {
    if summaries.is_empty() {
        tracing::warn!(
            execution_id = %execution_id,
            target = %request.target_context_id,
            version = request.array_version,
            "compression replacement carries no summaries: cancelling"
        );
        return CompressionSignalOutcome::Vetoed {
            reason: "empty compression replacement".to_string(),
        };
    }
    let Some(bus) = bus else {
        tracing::warn!(
            execution_id = %execution_id,
            target = %request.target_context_id,
            version = request.array_version,
            "compression replacement has no event bus to publish on: cancelling"
        );
        return CompressionSignalOutcome::Vetoed {
            reason: "compression replacement without event bus".to_string(),
        };
    };
    let tokens_after: u64 = summaries
        .iter()
        .map(|message| wf_llm::count::estimate_message_tokens(message) as u64)
        .sum();
    let run_id = wf_common::generate_id();
    let mut completed = crate::build_context_compression_completed_event(
        &execution_id.to_string(),
        agent_loop_id.map(|id| id.to_string()).as_deref(),
        &crate::ContextCompressionCompleted {
            target_context_id: request.target_context_id,
            array_version: request.array_version,
            tokens_after,
            messages: Some(&summaries),
            tail_keep: 0,
            degraded: false,
            still_over_budget: request.token_limit > 0 && tokens_after > request.token_limit,
            blocks: &[],
        },
    );
    crate::set_compression_run_id(&mut completed, &run_id);
    if let Err(error) = bus.publish(completed) {
        tracing::error!(
            execution_id = %execution_id,
            target = %request.target_context_id,
            version = request.array_version,
            error = %error,
            "compression replacement completion publish failed"
        );
    }
    CompressionSignalOutcome::Taken
}

/// Anchor check shared by every versioned write-back path: the result only
/// applies while the array is still at the version the result was produced
/// from; concurrent appends win over stale results.
pub fn check_anchor(current_version: u64, anchor_version: u64) -> bool {
    current_version == anchor_version
}

/// Fresh per-request overhead beyond the stable array estimate (tool
/// declarations, injected blocks, assembly-time mutations). Computed from
/// the assembled request so it never accumulates into the ledger.
pub fn dynamic_request_overhead(request_estimate: u64, stable_estimate: u64) -> u64 {
    request_estimate.saturating_sub(stable_estimate)
}

/// Whether an in-flight compression flight started at `started_at_ms` has
/// exceeded `timeout_ms` at `now_ms`. Flights carry their emission time so a
/// lost completion without a failure event cannot block emitters forever.
pub fn flight_expired(started_at_ms: i64, now_ms: i64, timeout_ms: u64) -> bool {
    now_ms.saturating_sub(started_at_ms) as u64 > timeout_ms
}

/// Shared no-taker escalation log for compression signals without a
/// service taker. Single writer so agent and workflow emitters escalate
/// on the same streak with the same wording; the audit event is kept and
/// backpressure stays unanchored on every path.
pub fn report_no_taker(target: &str, version: u64, streak: u32) {
    if streak >= crate::token_tracker::TokenUsageTracker::NO_TAKER_ESCALATION_THRESHOLD {
        tracing::error!(
            target = %target,
            version = version,
            streak = streak,
            "compression signal has no taker; audit event kept with no_taker marker, flight not anchored"
        );
    } else {
        tracing::warn!(
            target = %target,
            version = version,
            streak = streak,
            "compression signal has no taker; audit event kept with no_taker marker, flight not anchored"
        );
    }
}

/// Terminal state of one compression settle wait. Version advance means the
/// write-back landed, flight release without advance means the anchor went
/// away on its own, failure carries the terminal message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressionSettleOutcome {
    VersionAdvanced,
    FlightReleased,
    Failed { message: String },
    Timeout,
    Cancelled,
}

/// Unified settle wait for every compression emitter. Polls the version
/// anchor, the terminal failure and the flight release on one cadence and
/// races the whole wait against cancellation. Success wins over failure
/// when both appear together, matching the historical emitter behavior.
#[allow(clippy::too_many_arguments)]
pub async fn settle_compression_anchor<V, VFut, R, RFut, F, FFut>(
    anchor: u64,
    timeout_ms: u64,
    cancel: Option<tokio_util::sync::CancellationToken>,
    mut current_version: V,
    mut is_flight_released: R,
    mut find_failure: F,
) -> CompressionSettleOutcome
where
    V: FnMut() -> VFut,
    VFut: std::future::Future<Output = u64>,
    R: FnMut() -> RFut,
    RFut: std::future::Future<Output = bool>,
    F: FnMut() -> FFut,
    FFut: std::future::Future<Output = Option<String>>,
{
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let poll = std::time::Duration::from_millis(crate::token_events::COMPRESSION_SETTLE_POLL_MS);
    loop {
        if current_version().await != anchor {
            return CompressionSettleOutcome::VersionAdvanced;
        }
        if let Some(message) = find_failure().await {
            return CompressionSettleOutcome::Failed { message };
        }
        if is_flight_released().await {
            return CompressionSettleOutcome::FlightReleased;
        }
        if tokio::time::Instant::now() >= deadline {
            return CompressionSettleOutcome::Timeout;
        }
        match cancel.clone() {
            Some(token) => {
                tokio::select! {
                    _ = tokio::time::sleep(poll) => {}
                    _ = token.cancelled() => {
                        return CompressionSettleOutcome::Cancelled;
                    }
                }
            }
            None => tokio::time::sleep(poll).await,
        }
    }
}

/// Apply the actual-minus-estimated bias to a stable array estimate. Single
/// home for the bias composition so trackers and hot helpers agree.
pub fn apply_calibration_bias(stable_estimate: u64, bias: i64) -> u64 {
    if bias >= 0 {
        stable_estimate.saturating_add(bias as u64)
    } else {
        stable_estimate.saturating_sub((-bias) as u64)
    }
}

/// Stable projection from a read projection: the single source for every
/// budget decision. Callers pass the projected messages (the view the LLM
/// request is assembled from); the ledger estimate is never read for a
/// decision and serves only as a versioned guard alongside the count.
pub fn stable_projection(messages: &[Message]) -> u64 {
    wf_llm::count::estimate_messages(messages) as u64
}

/// Hot estimate from a stable projection plus an already-shared dynamic
/// overhead. Single home for the bias synthesis so single-target and
/// multi-target emitters agree on the suffix of the formula.
pub fn hot_estimate_with_shared_dynamic(stable: u64, shared_dynamic: u64, bias: i64) -> u64 {
    apply_calibration_bias(stable.saturating_add(shared_dynamic), bias)
}

/// Canonical hot estimate from one stable projection and one assembled
/// request estimate. The dynamic overhead never accumulates into the ledger.
/// Delegates to the shared-dynamic form so there is exactly one bias
/// composition in the codebase.
pub fn request_hot_estimate(stable_projection: u64, request_estimate: u64, bias: i64) -> u64 {
    let dynamic = dynamic_request_overhead(request_estimate, stable_projection);
    hot_estimate_with_shared_dynamic(stable_projection, dynamic, bias)
}

/// Array-dimension emission guard over a versioned ledger. Single home so
/// the session single-array adapter and the variable-map multi-array adapter
/// share the threshold-free guard logic; threshold comparison stays with the
/// hot helpers above.
pub fn array_should_emit(
    ledger: &wf_types::llm::TokenLedger,
    target: &str,
    version: u64,
) -> bool {
    ledger.should_emit(target, version)
}

/// Record an emission for one array version. Single home next to the guard.
pub fn array_mark_emitted(
    ledger: &mut wf_types::llm::TokenLedger,
    target: &str,
    version: u64,
) {
    ledger.mark_emitted(target, version);
}

/// Version of one named array. Single home so callers never reach into the
/// ledger map directly.
pub fn array_version_of(ledger: &wf_types::llm::TokenLedger, target: &str) -> u64 {
    ledger.version(target)
}

/// Final gate for one hot sample: a forced safety-net emission always
/// passes, otherwise the hot value must exceed the context budget.
/// Preflight refinement never changes this branch; it only sharpens the
/// warning text.
pub fn should_emit_hot(hot: u64, context_limit: u64, forced: bool) -> bool {
    forced || over_budget(hot, context_limit)
}

/// Shared parking rule from a settle terminal to the externally perceivable
/// message. `None` means continue (`VersionAdvanced`) or propagate
/// cancellation (`Cancelled`); every other terminal maps to external
/// handling so agent pause and workflow classified-failure share one rule.
pub fn settle_parking_message(
    target: &str,
    version: u64,
    outcome: &CompressionSettleOutcome,
) -> Option<String> {
    match outcome {
        CompressionSettleOutcome::VersionAdvanced => None,
        CompressionSettleOutcome::Cancelled => None,
        CompressionSettleOutcome::Failed { message } => Some(format!(
            "context compression for '{target}' at version {version} failed: {message}; pausing for external handling"
        )),
        CompressionSettleOutcome::FlightReleased => Some(format!(
            "context compression for '{target}' at version {version} released without version advance; pausing for external handling"
        )),
        CompressionSettleOutcome::Timeout => Some(format!(
            "context compression for '{target}' at version {version} timed out; pausing for external handling"
        )),
    }
}

/// One assembled emission: the shared entry every regular and forced path
/// funnels through. The caller supplies the already-chosen hot value, the
/// snapshot and the plan; this entry builds the request, delivers the
/// signal synchronously, stamps the audit copy and returns both so the
/// caller only stores its own pending state and anchors its own flight.
#[allow(clippy::too_many_arguments)]
pub struct UnifiedEmission<'a> {
    pub target: &'a str,
    pub tokens_used: u64,
    pub token_limit: u64,
    pub array_version: u64,
    pub forced: bool,
    pub messages: &'a [Message],
    pub plan: &'a [crate::CompactionPlanItem],
    pub directive: &'a str,
    pub execution_id: &'a Id,
    pub agent_loop_id: Option<&'a Id>,
    pub static_defs: &'a [crate::hooks::HookDefinition],
}

/// Deliver one assembled emission and return the outcome plus the annotated
/// audit event. Backpressure anchors only on `Taken`; vetoed clears the
/// pending plan without a no-taker streak and untaken keeps the audit with
/// the no-taker marker without anchoring. Forced and regular paths share
/// this wording and the same safety-net note.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_unified_emission(
    registry: Option<&HookHandlerRegistry>,
    bus: Option<&EventBus>,
    emission: &UnifiedEmission<'_>,
    cancellation: tokio_util::sync::CancellationToken,
) -> (CompressionSignalOutcome, BaseEvent) {
    let request = compression_request(
        emission.target,
        emission.tokens_used,
        emission.token_limit,
        emission.messages.len(),
        emission.array_version,
        emission.forced,
        emission.messages,
        emission.plan,
        emission.directive,
    );
    let outcome = dispatch_compression_signal(
        registry,
        bus,
        emission.execution_id,
        emission.agent_loop_id,
        cancellation,
        &request,
        emission.static_defs,
    )
    .await;
    let mut audit = compression_event(
        &emission.execution_id.to_string(),
        emission.agent_loop_id.map(|id| id.to_string()).as_deref(),
        &request,
    );
    annotate_compression_audit(&mut audit, &outcome);
    (outcome, audit)
}

/// Refined whole-request estimate for preflight warnings. Uses the local
/// estimate below the precision threshold and the provider count when near
/// or over budget. Warning only, never blocks the request.
pub async fn refine_request_estimate(
    gateway: &LlmGateway,
    request: &LlmRequest,
    context_limit: u64,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> u64 {
    let mut estimated = u64::from(wf_llm::estimate_request_tokens(request));
    if context_limit > 0 && estimated as f64 > context_limit as f64 * 0.8 {
        if let Ok(count) = gateway.count_tokens(request, cancel).await {
            estimated = u64::from(count.input_tokens);
        }
    }
    estimated
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::{MessageContentValue, MessageRole};

    fn msg(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_common::generate_id(),
            role,
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

    #[test]
    fn over_budget_respects_disabled_limit() {
        assert!(!over_budget(10_000, 0));
        assert!(!over_budget(999, 1000));
        assert!(over_budget(1001, 1000));
    }

    #[test]
    fn writeback_op_resolves_wire_names() {
        assert_eq!(
            WritebackOp::from_operation_name(crate::WRITEBACK_OPERATION_APPEND),
            Some(WritebackOp::Append)
        );
        assert_eq!(WritebackOp::from_operation_name("bogus"), None);
        assert_eq!(
            WritebackOp::from_operation_name("replace"),
            None,
            "replace wire name is rejected: history is append-only"
        );
    }

    #[test]
    fn compression_request_carries_snapshot() {
        let messages = vec![msg(MessageRole::User, "hi")];
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[], "");
        assert_eq!(request.target_context_id, "chat");
        assert_eq!(request.array_version, 7);
        assert!(!request.forced);
    }

    #[test]
    fn dynamic_overhead_never_goes_negative() {
        assert_eq!(dynamic_request_overhead(1200, 1000), 200);
        assert_eq!(dynamic_request_overhead(800, 1000), 0);
    }

    #[test]
    fn flight_expiry_uses_timeout() {
        assert!(!flight_expired(1000, 1000, 60_000));
        assert!(flight_expired(0, 60_001, 60_000));
    }

    struct VetoHandler {
        reason: &'static str,
    }

    #[async_trait::async_trait]
    impl crate::hooks::HookHandler for VetoHandler {
        fn name(&self) -> &str {
            "test-veto"
        }
        async fn on_point(&self, _ctx: &crate::hooks::HookContext) -> crate::hooks::HookOutcome {
            crate::hooks::HookOutcome::Veto {
                reason: self.reason.to_string(),
            }
        }
    }

    struct TakeoverDouble {
        calls: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    #[async_trait::async_trait]
    impl crate::hooks::HookHandler for TakeoverDouble {
        fn name(&self) -> &str {
            crate::token_events::COMPRESSION_TAKEOVER_HANDLER_NAME
        }
        async fn on_point(&self, _ctx: &crate::hooks::HookContext) -> crate::hooks::HookOutcome {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            crate::hooks::HookOutcome::Continue
        }
    }

    struct ReplaceDouble;

    #[async_trait::async_trait]
    impl crate::hooks::HookHandler for ReplaceDouble {
        fn name(&self) -> &str {
            "test-replace"
        }
        async fn on_point(&self, _ctx: &crate::hooks::HookContext) -> crate::hooks::HookOutcome {
            crate::hooks::HookOutcome::Replace {
                summaries: vec![msg(MessageRole::User, "supplied summary")],
            }
        }
    }

    fn dispatch_registry(
        calls: std::sync::Arc<std::sync::atomic::AtomicU32>,
    ) -> HookHandlerRegistry {
        let registry = HookHandlerRegistry::new();
        assert!(registry.register_named(std::sync::Arc::new(TakeoverDouble { calls })));
        registry
    }

    #[tokio::test]
    async fn veto_cancels_before_takeover() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let registry = dispatch_registry(calls.clone());
        registry.register(
            crate::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
            std::sync::Arc::new(VetoHandler {
                reason: "policy says no",
            }),
            0,
        );
        let bus = EventBus::new(16);
        let messages = vec![msg(MessageRole::User, "hi")];
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[], "");
        let outcome = dispatch_compression_signal(
            Some(&registry),
            Some(&bus),
            &Id::from("exec-1".to_string()),
            None,
            tokio_util::sync::CancellationToken::new(),
            &request,
            &[],
        )
        .await;
        assert_eq!(
            outcome,
            CompressionSignalOutcome::Vetoed {
                reason: "policy says no".to_string()
            }
        );
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "veto must cancel before takeover"
        );
    }

    #[tokio::test]
    async fn takeover_runs_without_veto() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let registry = dispatch_registry(calls.clone());
        let bus = EventBus::new(16);
        let messages = vec![msg(MessageRole::User, "hi")];
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[], "");
        let outcome = dispatch_compression_signal(
            Some(&registry),
            Some(&bus),
            &Id::from("exec-1".to_string()),
            None,
            tokio_util::sync::CancellationToken::new(),
            &request,
            &[],
        )
        .await;
        assert_eq!(outcome, CompressionSignalOutcome::Taken);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn replace_publishes_completion_without_takeover() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let registry = dispatch_registry(calls.clone());
        registry.register(
            crate::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
            std::sync::Arc::new(ReplaceDouble),
            0,
        );
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();
        let messages = vec![msg(MessageRole::User, "hi")];
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[], "");
        let outcome = dispatch_compression_signal(
            Some(&registry),
            Some(&bus),
            &Id::from("exec-1".to_string()),
            None,
            tokio_util::sync::CancellationToken::new(),
            &request,
            &[],
        )
        .await;
        assert_eq!(outcome, CompressionSignalOutcome::Taken);
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "replacement skips the takeover"
        );
        let event = loop {
            match sub.try_recv() {
                Ok(event)
                    if event.r#type == wf_types::events::EventType::ContextCompressionCompleted =>
                {
                    break event;
                }
                // The fire's audit copy lands first; skip it.
                Ok(_) => continue,
                Err(error) => panic!("replacement completion must publish: {error}"),
            }
        };
        let supplied: Vec<Message> = serde_json::from_value(
            event
                .metadata
                .as_ref()
                .and_then(|meta| meta.get(crate::token_events::KEY_MESSAGES))
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .expect("completion carries the supplied summaries");
        assert_eq!(supplied.len(), 1);
        assert!(
            matches!(&supplied[0].content, MessageContentValue::Text(text) if text == "supplied summary")
        );
    }

    fn static_def(hook_type: &str, handler: Option<&str>, condition: Option<&str>) -> crate::hooks::HookDefinition {
        crate::hooks::HookDefinition {
            id: wf_types::Id::new(),
            hook_type: hook_type.to_string(),
            priority: 0,
            condition: condition.map(String::from),
            enabled: true,
            payload: None,
            handler: handler.map(String::from),
            create_checkpoint: None,
            checkpoint_description: None,
        }
    }

    #[tokio::test]
    async fn static_veto_cancels_before_takeover() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let registry = dispatch_registry(calls.clone());
        assert!(registry.register_named(std::sync::Arc::new(VetoHandler {
            reason: "static policy says no",
        })));
        let bus = EventBus::new(16);
        let messages = vec![msg(MessageRole::User, "hi")];
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[], "");
        let defs = vec![static_def(
            crate::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
            Some("test-veto"),
            None,
        )];
        // Reuse the named veto handler through a static definition: the
        // fire resolves the handler name and aggregates its veto.
        let outcome = dispatch_compression_signal(
            Some(&registry),
            Some(&bus),
            &Id::from("exec-1".to_string()),
            None,
            tokio_util::sync::CancellationToken::new(),
            &request,
            &defs,
        )
        .await;
        assert_eq!(
            outcome,
            CompressionSignalOutcome::Vetoed {
                reason: "static policy says no".to_string()
            }
        );
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "static veto must cancel before takeover"
        );
    }

    #[tokio::test]
    async fn static_condition_filters_before_notification() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let registry = dispatch_registry(calls.clone());
        assert!(registry.register_named(std::sync::Arc::new(VetoHandler {
            reason: "filtered veto",
        })));
        let bus = EventBus::new(16);
        let messages = vec![msg(MessageRole::User, "hi")];
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[], "");
        let defs = vec![static_def(
            crate::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
            Some("test-veto"),
            Some("missing_flag"),
        )];
        let outcome = dispatch_compression_signal(
            Some(&registry),
            Some(&bus),
            &Id::from("exec-1".to_string()),
            None,
            tokio_util::sync::CancellationToken::new(),
            &request,
            &defs,
        )
        .await;
        assert_eq!(outcome, CompressionSignalOutcome::Taken);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn static_missing_handler_fails_open() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let registry = dispatch_registry(calls.clone());
        let bus = EventBus::new(16);
        let messages = vec![msg(MessageRole::User, "hi")];
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[], "");
        let defs = vec![static_def(
            crate::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
            Some("not-registered"),
            None,
        )];
        let outcome = dispatch_compression_signal(
            Some(&registry),
            Some(&bus),
            &Id::from("exec-1".to_string()),
            None,
            tokio_util::sync::CancellationToken::new(),
            &request,
            &defs,
        )
        .await;
        assert_eq!(
            outcome,
            CompressionSignalOutcome::Taken,
            "unresolvable static handlers fail open at the gate"
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn calibration_bias_applies_both_directions() {
        assert_eq!(super::apply_calibration_bias(1000, 50), 1050);
        assert_eq!(super::apply_calibration_bias(1000, -200), 800);
        assert_eq!(super::apply_calibration_bias(10, -20), 0);
    }

    #[test]
    fn hot_estimate_combines_overhead_and_bias() {
        assert_eq!(super::request_hot_estimate(1000, 1200, 0), 1200);
        assert_eq!(super::request_hot_estimate(1000, 800, 0), 1000);
        assert_eq!(super::apply_calibration_bias(1200, 50), 1250);
    }

    #[test]
    fn same_snapshot_same_hot_across_emitters() {
        let messages = vec![msg(MessageRole::User, "hello world")];
        let stable = super::stable_projection(&messages);
        let request_estimate = stable.saturating_add(200);
        let via_request = super::request_hot_estimate(stable, request_estimate, 50);
        let shared = super::dynamic_request_overhead(request_estimate, stable);
        let via_shared = super::hot_estimate_with_shared_dynamic(stable, shared, 50);
        assert_eq!(via_request, via_shared);
        let ledger_like = stable;
        let via_ledger_shape =
            super::hot_estimate_with_shared_dynamic(ledger_like, shared, 50);
        assert_eq!(via_request, via_ledger_shape);
    }

    #[test]
    fn dual_backend_guard_agrees_on_same_sequence() {
        use wf_types::llm::TokenLedger;
        let mut session_ledger = TokenLedger::default();
        let mut map_ledger = TokenLedger::default();
        for version in [1u64, 2, 3] {
            assert_eq!(
                super::array_should_emit(&session_ledger, "chat", version),
                super::array_should_emit(&map_ledger, "chat", version)
            );
            super::array_mark_emitted(&mut session_ledger, "chat", version);
            super::array_mark_emitted(&mut map_ledger, "chat", version);
            assert!(!super::array_should_emit(&session_ledger, "chat", version));
            assert!(!super::array_should_emit(&map_ledger, "chat", version));
            assert_eq!(
                super::array_version_of(&session_ledger, "chat"),
                super::array_version_of(&map_ledger, "chat")
            );
        }
    }

    #[test]
    fn five_settle_terminals_map_once() {
        use super::CompressionSettleOutcome;
        assert!(super::settle_parking_message("chat", 7, &CompressionSettleOutcome::VersionAdvanced).is_none());
        assert!(super::settle_parking_message("chat", 7, &CompressionSettleOutcome::Cancelled).is_none());
        for outcome in [
            CompressionSettleOutcome::Failed { message: "boom".to_string() },
            CompressionSettleOutcome::FlightReleased,
            CompressionSettleOutcome::Timeout,
        ] {
            let text = super::settle_parking_message("chat", 7, &outcome).expect("parks");
            assert!(text.contains("chat") && text.contains('7'));
        }
    }

    #[test]
    fn untaken_and_veto_audit_markers_differ() {
        let messages = vec![msg(MessageRole::User, "hi")];
        let request = super::compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[], "");
        let mut vetoed = super::compression_event("exec-1", None, &request);
        super::annotate_compression_audit(&mut vetoed, &super::CompressionSignalOutcome::Vetoed { reason: "no".to_string() });
        let mut untaken = super::compression_event("exec-1", None, &request);
        super::annotate_compression_audit(&mut untaken, &super::CompressionSignalOutcome::Untaken);
        let veto_cancelled = vetoed.metadata.as_ref().and_then(|m| m.get(crate::token_events::KEY_CANCELLED)).is_some();
        let untaken_flag = untaken.metadata.as_ref().and_then(|m| m.get(crate::token_events::KEY_NO_TAKER)).is_some();
        assert!(veto_cancelled && untaken_flag);
    }

    #[tokio::test]
    async fn settle_reports_version_advance() {
        let outcome = super::settle_compression_anchor(
            7,
            1000,
            None,
            || async { 8 },
            || async { false },
            || async { None },
        )
        .await;
        assert_eq!(
            outcome,
            super::CompressionSettleOutcome::VersionAdvanced
        );
    }

    #[tokio::test]
    async fn settle_reports_failure_before_release() {
        let outcome = super::settle_compression_anchor(
            7,
            1000,
            None,
            || async { 7 },
            || async { true },
            || async { Some("boom".to_string()) },
        )
        .await;
        assert_eq!(
            outcome,
            super::CompressionSettleOutcome::Failed {
                message: "boom".to_string()
            }
        );
    }

    #[tokio::test]
    async fn settle_reports_release_without_version() {
        let outcome = super::settle_compression_anchor(
            7,
            1000,
            None,
            || async { 7 },
            || async { true },
            || async { None },
        )
        .await;
        assert_eq!(outcome, super::CompressionSettleOutcome::FlightReleased);
    }

    #[tokio::test]
    async fn settle_times_out_without_progress() {
        let outcome =
            super::settle_compression_anchor(7, 30, None, || async { 7 }, || async { false }, || async {
                None
            })
            .await;
        assert_eq!(outcome, super::CompressionSettleOutcome::Timeout);
    }

    #[tokio::test]
    async fn settle_observes_cancellation() {
        let token = tokio_util::sync::CancellationToken::new();
        let waiter = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            waiter.cancel();
        });
        let outcome = super::settle_compression_anchor(
            7,
            1000,
            Some(token),
            || async {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                7
            },
            || async { false },
            || async { None },
        )
        .await;
        assert_eq!(outcome, super::CompressionSettleOutcome::Cancelled);
    }
}
