use serde_json::Value;
use wf_core::EventBus;
use wf_types::events::BaseEvent;
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
pub async fn dispatch_compression_signal(
    registry: Option<&HookHandlerRegistry>,
    bus: Option<&EventBus>,
    execution_id: &Id,
    agent_loop_id: Option<&Id>,
    cancellation: tokio_util::sync::CancellationToken,
    request: &crate::ContextCompressionRequest<'_>,
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
        &[],
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

/// Blocking backpressure wait: poll `current_version` until it moves past
/// `anchor` (the compression write-back landed) or `timeout_ms` elapses.
/// Returns true when the version shifted, false on timeout. Callers race
/// this against their cancellation signal and against the compression
/// failure event; timeout and failure stop the emitting execution for manual
/// handling.
pub async fn wait_for_version_shift<F, Fut>(
    mut current_version: F,
    anchor: u64,
    timeout_ms: u64,
) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = u64>,
{
    let start = std::time::Instant::now();
    loop {
        if current_version().await != anchor {
            return true;
        }
        if start.elapsed().as_millis() as u64 >= timeout_ms {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(
            crate::token_events::COMPRESSION_SETTLE_POLL_MS,
        ))
        .await;
    }
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
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[]);
        assert_eq!(request.target_context_id, "chat");
        assert_eq!(request.array_version, 7);
        assert!(!request.forced);
    }

    #[test]
    fn dynamic_overhead_never_goes_negative() {
        assert_eq!(dynamic_request_overhead(1200, 1000), 200);
        assert_eq!(dynamic_request_overhead(800, 1000), 0);
    }

    #[tokio::test]
    async fn version_shift_wait_settles() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;
        let version = Arc::new(AtomicU64::new(7));
        let probe = version.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            probe.store(8, Ordering::SeqCst);
        });
        let moved = version.clone();
        assert!(
            wait_for_version_shift(
                move || {
                    let moved = moved.clone();
                    async move { moved.load(Ordering::SeqCst) }
                },
                7,
                1000,
            )
            .await
        );
        assert_eq!(version.load(Ordering::SeqCst), 8);
    }

    #[tokio::test]
    async fn version_shift_wait_times_out() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;
        let version = Arc::new(AtomicU64::new(7));
        let moved = version.clone();
        assert!(
            !wait_for_version_shift(
                move || {
                    let moved = moved.clone();
                    async move { moved.load(Ordering::SeqCst) }
                },
                7,
                30,
            )
            .await
        );
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
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[]);
        let outcome = dispatch_compression_signal(
            Some(&registry),
            Some(&bus),
            &Id::from("exec-1".to_string()),
            None,
            tokio_util::sync::CancellationToken::new(),
            &request,
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
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[]);
        let outcome = dispatch_compression_signal(
            Some(&registry),
            Some(&bus),
            &Id::from("exec-1".to_string()),
            None,
            tokio_util::sync::CancellationToken::new(),
            &request,
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
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages, &[]);
        let outcome = dispatch_compression_signal(
            Some(&registry),
            Some(&bus),
            &Id::from("exec-1".to_string()),
            None,
            tokio_util::sync::CancellationToken::new(),
            &request,
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
}
