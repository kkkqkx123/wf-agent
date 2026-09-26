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
pub fn compression_request<'a>(
    target_context_id: &'a str,
    tokens_used: u64,
    token_limit: u64,
    message_count: usize,
    array_version: u64,
    forced: bool,
    messages: &'a [Message],
) -> crate::ContextCompressionRequest<'a> {
    crate::ContextCompressionRequest {
        target_context_id,
        tokens_used,
        token_limit,
        message_count,
        array_version,
        forced,
        messages,
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

/// Deliver the compression signal synchronously to registered receivers
/// (the compression service takes over immediately). Agent targets attach
/// their loop id so the agent conversation self-consumes the completed
/// event; workflow targets carry no loop id and write back through the
/// execution registry.
///
/// Returns true when a registry was present and the signal was dispatched.
/// Returns false when no registry can take over: callers keep the audit
/// event but must not anchor backpressure, and should warn so the dropped
/// compression is explicit instead of silent.
pub async fn dispatch_compression_signal(
    registry: Option<&HookHandlerRegistry>,
    bus: Option<&EventBus>,
    execution_id: &Id,
    agent_loop_id: Option<&Id>,
    cancellation: tokio_util::sync::CancellationToken,
    request: &crate::ContextCompressionRequest<'_>,
) -> bool {
    let Some(registry) = registry else {
        tracing::warn!(
            execution_id = %execution_id,
            target = %request.target_context_id,
            version = request.array_version,
            "compression signal dropped: no hook registry to take over"
        );
        return false;
    };
    let mut data = crate::compression_request_hook_data(request);
    if let Some(loop_id) = agent_loop_id {
        data.insert(
            "agent_loop_id".to_string(),
            Value::String(loop_id.to_string()),
        );
    }
    fire(
        registry,
        &[],
        crate::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        &HookContext {
            execution_id: execution_id.clone(),
            hook_type: crate::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
            data,
            cancellation,
        },
        bus,
    )
    .await;
    true
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

/// Apply a versioned write-back to a plain message vector. Returns true
/// when the write took effect (anchor matched), false when the stale result
/// was discarded. Backends holding ledgers (agent session, workflow
/// variable map) perform their own ledger bookkeeping around this.
pub fn apply_to_vec(
    current: &mut Vec<Message>,
    current_version: u64,
    anchor_version: u64,
    operation: WritebackOp,
    messages: Vec<Message>,
) -> bool {
    if !check_anchor(current_version, anchor_version) {
        return false;
    }
    match operation {
        WritebackOp::Append => current.extend(messages),
    }
    true
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
    fn apply_to_vec_discards_stale_results() {
        let mut current = vec![
            msg(MessageRole::User, "old"),
            msg(MessageRole::User, "newer"),
        ];
        assert!(!apply_to_vec(
            &mut current,
            4,
            3,
            WritebackOp::Append,
            vec![msg(MessageRole::Assistant, "stale")],
        ));
        assert_eq!(current.len(), 2);
    }

    #[test]
    fn compression_request_carries_snapshot() {
        let messages = vec![msg(MessageRole::User, "hi")];
        let request = compression_request("chat", 1200, 1000, 1, 7, false, &messages);
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
}
