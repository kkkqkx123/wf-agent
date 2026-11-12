//! Unified hook fire: evaluation → payload resolution → ordered
//! notification → outcome aggregation → audit publication.
//!
//! The engine calls [`fire`] at a hook point and awaits it: the
//! notification barrier completes before the engine moves on. The pipeline
//! itself carries no behavior — filtering (condition / enabled / priority),
//! payload resolution and ordered notification only; behavior lives in
//! registered [`crate::hooks::handler::HookHandler`]s. Notification uses one global priority order
//! across static and dynamic handlers so a high-priority dynamic handler
//! can precede a low-priority static one.

use serde_json::Value;
use tracing::warn;
use wf_core::EventBus;
use wf_types::message::Message;

use crate::hooks::audit::{
    evaluate_hook_condition, filter_and_sort_hooks, publish_hook_audit_event,
};
use crate::hooks::registry::HookHandlerRegistry;
use crate::hooks::template::resolve_payload_template;
use crate::hooks::types::{HookContext, HookDefinition, HookOutcome};

/// Outcome of one handler notification.
#[derive(Debug, Clone)]
pub struct HandlerResult {
    pub name: String,
    /// Static definition this notification was resolved from, if any.
    /// Dynamically registered type handlers carry no definition.
    pub hook_id: Option<String>,
    pub outcome: HookOutcome,
    pub duration_ms: i64,
    /// Panic / unresolvable handler description; `None` on success.
    pub error: Option<String>,
}

/// Aggregate result of one fire: everything the audit trail needs
/// (payloads, per-handler results, duration) plus the aggregated outcome.
///
/// The outcome is `Veto` when at least one notified handler vetoed (reasons
/// joined in notification order, and any replacement is dropped); otherwise
/// `Replace` when a handler supplied one (first in notification order);
/// otherwise `Continue`. Only gate points act on a veto and only the
/// compression signal acts on a replacement; every other caller proceeds
/// as with `Continue`.
#[derive(Debug, Clone)]
pub struct FireSummary {
    pub hook_type: String,
    pub payloads: Vec<Value>,
    pub priorities: Vec<i32>,
    pub handler_results: Vec<HandlerResult>,
    /// Ids of the static definitions that passed evaluation (enabled +
    /// condition) for this fire, in priority order. Checkpoint opt-in is
    /// honored only for these ids: an opted-in definition whose condition
    /// failed requests no snapshot. Handlerless definitions are included
    /// (a pure snapshot request needs no handler); dynamically registered
    /// type handlers carry no definition and never appear here.
    pub matched_hook_ids: Vec<String>,
    pub duration_ms: i64,
    pub outcome: HookOutcome,
    /// Supplied compression replacement (first `Replace` in notification
    /// order), honored only by the compression signal dispatch; `None`
    /// unless a handler replaced. A veto anywhere clears this: denial wins
    /// over supplied content.
    pub replacement: Option<Vec<Message>>,
}

impl FireSummary {
    /// The veto reason when the fire denied the guarded step, `None` on
    /// `Continue`. Gate points check this; non-gate callers ignore it.
    pub fn vetoed_reason(&self) -> Option<&str> {
        match &self.outcome {
            HookOutcome::Veto { reason } => Some(reason.as_str()),
            HookOutcome::Continue | HookOutcome::Replace { .. } => None,
        }
    }

    /// Per-handler veto reasons in notification order. The aggregated
    /// outcome joins these, but gate callers needing attribution should
    /// read this list alongside `handler_results`.
    pub fn vetoed_reasons(&self) -> Vec<&str> {
        self.handler_results
            .iter()
            .filter_map(|r| match &r.outcome {
                HookOutcome::Veto { reason } => Some(reason.as_str()),
                HookOutcome::Continue | HookOutcome::Replace { .. } => None,
            })
            .collect()
    }

    /// Formatted gate rejection detail for the fired point, `None` on
    /// `Continue` and `None` at non-gate points (a veto there is observed,
    /// never blocking). Display-only: keeps the two gate paths on one wording
    /// so audit and user-facing rejections cannot drift apart. Callers needing
    /// attribution must use `vetoed_reasons` plus `handler_results`, never
    /// parse this string.
    pub fn gate_rejection_detail(&self) -> Option<String> {
        if !wf_types::hook::is_gate_hook(&self.hook_type) {
            return None;
        }
        self.vetoed_reason()
            .map(|reason| format!("hook veto at {}: {reason}", self.hook_type))
    }
}

/// Fire a hook point:
///
/// 1. statically evaluate the hook definitions of `hook_type`
///    (condition / enabled / priority filtering) and resolve payload templates;
/// 2. synchronously notify every handler that passes evaluation in one
///    global priority order: static `handler`-named definitions and
///    dynamically registered type handlers merge into a single queue sorted
///    by priority descending with stable insertion order for ties.
///    Priority never decides whether a handler runs; its only effect is the
///    notification order (hence the audit summary order and the veto-reason
///    join order). Both populations share one evaluation semantic: the same
///    condition language over the same context data, and a failing condition
///    skips the handler, never the engine;
/// 3. publish the `HOOK_TRIGGERED` audit event carrying the payloads and the
///    per-handler results. The aggregated outcome is `Veto` when at least
///    one notified handler vetoed (any replacement is dropped with it);
///    otherwise `Replace` when a handler supplied one; only gate points act
///    on a veto and only the compression signal dispatch consumes a
///    replacement (`Continue` otherwise, including panicking, timed-out and
///    unresolvable handlers: gates fail open on infrastructure gaps; a
///    handler that wants to deny on its own slow path returns the `Veto`
///    itself).
///
/// Ordering guarantee: every handler settles (panics, timeouts and
/// cancellations are contained by the registry) before the audit event is
/// published, so a trigger template matching the audit event always starts
/// after the synchronous handlers. The engine awaits the handler barrier
/// but never waits for trigger execution: trigger completion is unordered
/// relative to the engine's next step, and trigger effects must commute
/// with it.
///
/// Caller contract: `hook_type` must equal `ctx.hook_type`. A mismatch is
/// logged and resolved to the context type so filtering and the audit event
/// observe one point; callers must fix the call site.
pub async fn fire(
    registry: &HookHandlerRegistry,
    hooks: &[HookDefinition],
    hook_type: &str,
    ctx: &HookContext,
    event_bus: Option<&EventBus>,
) -> FireSummary {
    let hook_type = if hook_type != ctx.hook_type {
        tracing::error!(
            expected = %ctx.hook_type,
            actual = %hook_type,
            "fire hook_type mismatches context hook type; using context type for filtering and audit"
        );
        ctx.hook_type.as_str()
    } else {
        hook_type
    };
    let started = wf_common::now();

    let mut payloads: Vec<Value> = Vec::new();
    let mut priorities: Vec<i32> = Vec::new();
    let mut matched: Vec<HookDefinition> = Vec::new();
    for hook in filter_and_sort_hooks(hooks, hook_type) {
        match evaluate_hook_condition(hook.condition.as_deref(), &ctx.data) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => {
                warn!(
                    hook_id = %hook.id,
                    hook_type = %hook.hook_type,
                    "hook condition evaluation failed, skipping: {}",
                    e
                );
                continue;
            }
        }
        let payload = match &hook.payload {
            Some(template) => match resolve_payload_template(template, &ctx.data) {
                Ok(resolved) => resolved,
                Err(e) => {
                    warn!(
                        hook_id = %hook.id,
                        hook_type = %hook.hook_type,
                        "hook payload template resolution failed, using null: {}",
                        e
                    );
                    Value::Null
                }
            },
            None => Value::Null,
        };
        payloads.push(payload);
        priorities.push(hook.priority);
        matched.push(hook);
    }

    if registry.is_shared_fallback()
        && (matched.iter().any(|h| h.handler.is_some())
            || wf_types::hook::hook_requires_handler(hook_type))
    {
        warn!(
            hook_type = %hook_type,
            execution_id = %ctx.execution_id,
            "hook fired with the shared fallback registry while a handler is expected; running audit-only"
        );
    }

    enum PendingNotify {
        Ready(crate::hooks::registry::RegisteredHandler),
        Missing { name: String, hook_id: String },
    }

    struct OrderedNotify {
        priority: i32,
        order: usize,
        hook_id: Option<String>,
        pending: PendingNotify,
    }

    let mut ordered: Vec<OrderedNotify> = Vec::new();
    for def in &matched {
        let hook_id = def.id.to_string();
        let Some(name) = def.handler.as_deref() else {
            continue;
        };
        let pending = match registry.get(name) {
            Some(handler) => PendingNotify::Ready(crate::hooks::registry::RegisteredHandler {
                name: name.to_string(),
                priority: def.priority,
                handler,
                condition: None,
            }),
            None => PendingNotify::Missing {
                name: name.to_string(),
                hook_id: def.id.to_string(),
            },
        };
        ordered.push(OrderedNotify {
            priority: def.priority,
            order: ordered.len(),
            hook_id: Some(hook_id),
            pending,
        });
    }

    for registered in registry.for_type(hook_type) {
        match evaluate_hook_condition(registered.condition.as_deref(), &ctx.data) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => {
                warn!(
                    handler = %registered.name,
                    hook_type = %hook_type,
                    "hook handler condition evaluation failed, skipping: {}",
                    e
                );
                continue;
            }
        }
        let priority = registered.priority;
        ordered.push(OrderedNotify {
            priority,
            order: ordered.len(),
            hook_id: None,
            pending: PendingNotify::Ready(registered),
        });
    }

    ordered.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.order.cmp(&b.order))
    });

    let mut handler_results: Vec<HandlerResult> = Vec::new();
    for item in ordered {
        match item.pending {
            PendingNotify::Ready(registered) => {
                tracing::debug!(
                    handler = %registered.name,
                    hook_type = %hook_type,
                    "hook handler runs synchronously before the HOOK_TRIGGERED audit event; a matching trigger template starts after the handler barrier and is not awaited"
                );
                let mut result = registry.notify(ctx, &registered).await;
                result.hook_id = item.hook_id;
                handler_results.push(result);
            }
            PendingNotify::Missing { name, hook_id } => {
                if wf_types::hook::is_gate_hook(hook_type) {
                    tracing::error!(
                        hook_id = %hook_id,
                        handler = %name,
                        "hook handler '{}' is not registered, gate allows by fail-open; fix the handler name",
                        name
                    );
                } else {
                    warn!(
                        hook_id = %hook_id,
                        handler = %name,
                        "hook handler '{}' is not registered, skipping",
                        name
                    );
                }
                handler_results.push(HandlerResult {
                    name,
                    hook_id: Some(hook_id),
                    outcome: HookOutcome::Continue,
                    duration_ms: 0,
                    error: Some("handler not registered".to_string()),
                });
            }
        }
    }

    let duration_ms = wf_common::now() - started;
    let vetoes: Vec<&str> = handler_results
        .iter()
        .filter_map(|r| match &r.outcome {
            HookOutcome::Veto { reason } => Some(reason.as_str()),
            HookOutcome::Continue | HookOutcome::Replace { .. } => None,
        })
        .collect();
    // First replacement in notification order wins; any veto clears it:
    // denial is the safer direction, so denied content never applies.
    let replacement: Option<Vec<Message>> = handler_results.iter().find_map(|r| match &r.outcome {
        HookOutcome::Replace { summaries } => Some(summaries.clone()),
        HookOutcome::Continue | HookOutcome::Veto { .. } => None,
    });
    let (outcome, replacement) = if vetoes.is_empty() {
        match replacement {
            Some(summaries) => (
                HookOutcome::Replace {
                    summaries: summaries.clone(),
                },
                Some(summaries),
            ),
            None => (HookOutcome::Continue, None),
        }
    } else {
        (
            HookOutcome::Veto {
                reason: vetoes.join("; "),
            },
            None,
        )
    };
    // A replacement only takes effect at the compression signal; at every
    // other point it is recorded and ignored, mirroring vetoes at non-gate
    // points, so surface the likely misconfiguration loudly.
    if replacement.is_some() && hook_type != crate::token_events::COMPRESSION_SIGNAL_HOOK_TYPE {
        warn!(
            hook_type,
            "hook replace at a non-compression point: recorded and ignored"
        );
    }

    publish_hook_audit_event(
        event_bus,
        ctx,
        &payloads,
        &priorities,
        &handler_results,
        duration_ms,
    );

    let summary = FireSummary {
        hook_type: hook_type.to_string(),
        payloads,
        priorities,
        handler_results,
        matched_hook_ids: matched.iter().map(|h| h.id.clone()).collect(),
        duration_ms,
        outcome,
        replacement,
    };
    // A veto only takes effect at gate points; at every other point it is
    // recorded and ignored, so surface the likely misconfiguration loudly.
    if !wf_types::hook::is_gate_hook(hook_type) {
        if let HookOutcome::Veto { reason } = &summary.outcome {
            warn!(
                hook_type,
                reason = %reason,
                "hook veto at a non-gate point: recorded and ignored"
            );
        }
    }
    summary
}

#[cfg(test)]
mod tests;
