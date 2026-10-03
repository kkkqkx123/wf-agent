//! Unified hook fire: evaluation → payload resolution → ordered
//! notification → outcome aggregation → audit publication.
//!
//! The engine calls [`fire`] at a hook point and awaits it: the
//! notification barrier completes before the engine moves on. The pipeline
//! itself carries no behavior — filtering (condition / enabled / priority),
//! payload resolution and ordered notification only; behavior lives in
//! registered [`HookHandler`]s. Notification uses one global priority order
//! across static and dynamic handlers so a high-priority dynamic handler
//! can precede a low-priority static one.

use serde_json::Value;
use tracing::warn;
use wf_core::EventBus;

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
/// joined in notification order); only gate points act on it, every other
/// caller proceeds as with `Continue`.
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
}

impl FireSummary {
    /// The veto reason when the fire denied the guarded step, `None` on
    /// `Continue`. Gate points check this; non-gate callers ignore it.
    pub fn vetoed_reason(&self) -> Option<&str> {
        match &self.outcome {
            HookOutcome::Veto { reason } => Some(reason.as_str()),
            HookOutcome::Continue => None,
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
                HookOutcome::Continue => None,
            })
            .collect()
    }

    /// Formatted gate rejection detail for the fired point, `None` on
    /// `Continue` and `None` at non-gate points (a veto there is observed,
    /// never blocking). Keeps the two gate paths on one wording so audit and
    /// user-facing rejections cannot drift apart.
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
///    one notified handler vetoed; only gate points act on it (`Continue`
///    otherwise, including panicking, timed-out and unresolvable handlers:
///    gates fail open on infrastructure gaps; a handler that wants to deny
///    on its own slow path returns the `Veto` itself).
///
/// Ordering guarantee: every handler settles (panics, timeouts and
/// cancellations are contained by the registry) before the audit event is
/// published, so a trigger template matching the audit event always starts
/// after the synchronous handlers. The engine awaits the handler barrier
/// but never waits for trigger execution: trigger completion is unordered
/// relative to the engine's next step, and trigger effects must commute
/// with it.
///
/// Caller contract: `hook_type` must equal `ctx.hook_type`. Filtering uses
/// the argument while the audit event is published under the context type,
/// so a mismatch would filter one point and record another (debug builds
/// assert this).
pub async fn fire(
    registry: &HookHandlerRegistry,
    hooks: &[HookDefinition],
    hook_type: &str,
    ctx: &HookContext,
    event_bus: Option<&EventBus>,
) -> FireSummary {
    debug_assert_eq!(
        hook_type,
        ctx.hook_type,
        "fire hook_type must match the context hook type; audit is published under the context type"
    );
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
            HookOutcome::Continue => None,
        })
        .collect();
    let outcome = if vetoes.is_empty() {
        HookOutcome::Continue
    } else {
        HookOutcome::Veto {
            reason: vetoes.join("; "),
        }
    };

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
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use super::*;
    use crate::hooks::handler::HookHandler;
    use wf_types::Id;

    struct CounterHandler {
        name: &'static str,
        calls: Arc<AtomicU32>,
        outcome: HookOutcome,
    }

    #[async_trait::async_trait]
    impl HookHandler for CounterHandler {
        fn name(&self) -> &str {
            self.name
        }
        async fn on_point(&self, ctx: &HookContext) -> HookOutcome {
            assert!(!ctx.hook_type.is_empty(), "context carries the hook type");
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.outcome.clone()
        }
    }

    fn ctx() -> HookContext {
        ctx_of("TEST")
    }

    fn ctx_of(hook_type: &str) -> HookContext {
        HookContext {
            execution_id: Id::from("exec-1".to_string()),
            hook_type: hook_type.to_string(),
            data: HashMap::new(),
            cancellation: tokio_util::sync::CancellationToken::new(),
        }
    }

    fn hook_def(hook_type: &str, priority: i32, handler: Option<&str>) -> HookDefinition {
        HookDefinition {
            id: Id::new(),
            hook_type: hook_type.to_string(),
            priority,
            condition: None,
            enabled: true,
            payload: None,
            handler: handler.map(String::from),
            create_checkpoint: None,
            checkpoint_description: None,
        }
    }

    #[tokio::test]
    async fn registered_handler_is_notified_synchronously() {
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "TEST",
            Arc::new(CounterHandler {
                name: "r1",
                calls: calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
        );

        let summary = fire(&registry, &[], "TEST", &ctx(), None).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(summary.handler_results.len(), 1);
        assert_eq!(summary.handler_results[0].name, "r1");
        assert_eq!(summary.outcome, HookOutcome::Continue);
    }

    #[tokio::test]
    async fn handler_field_resolves_named_handler() {
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "OTHER",
            Arc::new(CounterHandler {
                name: "named",
                calls: calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
        );

        let hooks = vec![hook_def("TEST", 1, Some("named"))];
        let summary = fire(&registry, &hooks, "TEST", &ctx(), None).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(summary.handler_results.len(), 1);
        assert_eq!(summary.handler_results[0].name, "named");
    }

    #[tokio::test]
    async fn unresolvable_handler_is_reported_not_fatal() {
        let registry = HookHandlerRegistry::new();
        let hooks = vec![hook_def("TEST", 1, Some("missing"))];
        let summary = fire(&registry, &hooks, "TEST", &ctx(), None).await;
        assert_eq!(summary.handler_results.len(), 1);
        assert_eq!(summary.handler_results[0].name, "missing");
        assert!(summary.handler_results[0].error.is_some());
        assert_eq!(summary.outcome, HookOutcome::Continue);
    }

    #[tokio::test]
    async fn condition_filters_definition_before_notification() {
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "TEST",
            Arc::new(CounterHandler {
                name: "r1",
                calls: calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
        );

        // Dynamic handlers always run; the static definition is filtered.
        let hooks = vec![HookDefinition {
            id: Id::new(),
            hook_type: "TEST".to_string(),
            priority: 1,
            condition: Some("missing_flag".to_string()),
            enabled: true,
            payload: None,
            handler: Some("r1".to_string()),
            create_checkpoint: None,
            checkpoint_description: None,
        }];
        let summary = fire(&registry, &hooks, "TEST", &ctx(), None).await;
        assert_eq!(
            summary.handler_results.len(),
            1,
            "only the dynamic handler runs"
        );
        assert_eq!(summary.handler_results[0].name, "r1");
    }

    #[tokio::test]
    async fn outcome_is_continue_without_veto_and_priority_orders_notification() {
        let registry = HookHandlerRegistry::new();
        let continue_calls = Arc::new(AtomicU32::new(0));
        let other_calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "TEST",
            Arc::new(CounterHandler {
                name: "continue-r",
                calls: continue_calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
        );
        registry.register(
            "TEST",
            Arc::new(CounterHandler {
                name: "other-r",
                calls: other_calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            10,
        );

        let summary = fire(&registry, &[], "TEST", &ctx(), None).await;
        assert_eq!(summary.outcome, HookOutcome::Continue);
        // Notified in priority order.
        assert_eq!(summary.handler_results[0].name, "other-r");
        assert_eq!(summary.handler_results[1].name, "continue-r");
    }

    #[tokio::test]
    async fn veto_aggregates_and_surfaces_reason() {
        let registry = HookHandlerRegistry::new();
        struct VetoHandler;
        #[async_trait::async_trait]
        impl HookHandler for VetoHandler {
            fn name(&self) -> &str {
                "gate"
            }
            async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
                HookOutcome::Veto {
                    reason: "missing input file".to_string(),
                }
            }
        }
        registry.register("TEST", Arc::new(VetoHandler), 1);

        let summary = fire(&registry, &[], "TEST", &ctx(), None).await;
        assert_eq!(
            summary.vetoed_reason(),
            Some("missing input file"),
            "gate points read the veto reason off the summary"
        );
        assert!(summary.outcome.is_veto());
        assert_eq!(summary.handler_results[0].outcome.as_str(), "vetoed");
    }

    #[tokio::test]
    async fn dynamic_handler_condition_filters_like_static_definitions() {
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(AtomicU32::new(0));
        registry.register_with_condition(
            "TEST",
            Arc::new(CounterHandler {
                name: "gated",
                calls: calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
            Some("allow".to_string()),
        );

        // Mismatching context: the dynamic handler is skipped, same as a
        // static definition whose condition fails.
        let denied = fire(&registry, &[], "TEST", &ctx(), None).await;
        assert!(denied.handler_results.is_empty());
        assert_eq!(denied.vetoed_reason(), None);
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        // Matching context: notified.
        let mut data = HashMap::new();
        data.insert("allow".to_string(), Value::Bool(true));
        let allowed_ctx = HookContext {
            execution_id: Id::from("exec-1".to_string()),
            hook_type: "TEST".to_string(),
            data,
            cancellation: tokio_util::sync::CancellationToken::new(),
        };
        let allowed = fire(&registry, &[], "TEST", &allowed_ctx, None).await;
        assert_eq!(allowed.handler_results.len(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn handler_panic_is_contained_as_error_result() {
        let registry = HookHandlerRegistry::new();
        struct PanickingHandler;
        #[async_trait::async_trait]
        impl HookHandler for PanickingHandler {
            fn name(&self) -> &str {
                "boom"
            }
            async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
                panic!("handler blew up");
            }
        }
        registry.register("TEST", Arc::new(PanickingHandler), 1);

        let summary = fire(&registry, &[], "TEST", &ctx(), None).await;
        assert_eq!(summary.handler_results.len(), 1);
        assert!(summary.handler_results[0]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("handler blew up")));
        assert_eq!(
            summary.outcome,
            HookOutcome::Continue,
            "a panicking handler never vetoes and never aborts the fire"
        );
    }

    #[tokio::test]
    async fn handler_bounds_itself_with_context_cancellation() {
        // The pipeline has no timeout: a slow handler is expected to race
        // its own work against `ctx.cancellation`, so cancelling the
        // execution settles the fire instead of hanging it.
        let registry = HookHandlerRegistry::new();
        struct SlowHandler;
        #[async_trait::async_trait]
        impl HookHandler for SlowHandler {
            fn name(&self) -> &str {
                "slow"
            }
            async fn on_point(&self, ctx: &HookContext) -> HookOutcome {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => HookOutcome::Continue,
                    _ = ctx.cancellation.cancelled() => HookOutcome::Veto { reason: "execution cancelled mid-handler".to_string() },
                }
            }
        }
        registry.register("TEST", Arc::new(SlowHandler), 1);

        let ctx = ctx();
        ctx.cancellation.cancel();
        let summary = fire(&registry, &[], "TEST", &ctx, None).await;
        assert!(summary.outcome.is_veto());
    }

    #[tokio::test]
    async fn audit_event_published_with_results_summary() {
        use wf_core::EventBus;
        use wf_types::events::EventType;

        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "TEST",
            Arc::new(CounterHandler {
                name: "r1",
                calls: calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
        );

        let bus = Arc::new(EventBus::new(16));
        let mut sub = bus.subscribe();

        let hooks = vec![HookDefinition {
            id: Id::new(),
            hook_type: "TEST".to_string(),
            priority: 5,
            condition: None,
            enabled: true,
            payload: Some(serde_json::json!({"k": "{{name}}"})),
            handler: None,
            create_checkpoint: None,
            checkpoint_description: None,
        }];
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("world".to_string()));
        let ctx = HookContext {
            execution_id: Id::from("exec-1".to_string()),
            hook_type: "TEST".to_string(),
            data,
            cancellation: tokio_util::sync::CancellationToken::new(),
        };

        fire(&registry, &hooks, "TEST", &ctx, Some(&bus)).await;

        let event = sub.try_recv().expect("audit event must be published");
        assert_eq!(event.r#type, EventType::HookTriggered);
        assert_eq!(event.execution_id.as_deref(), Some("exec-1"));
        let metadata = event.metadata.as_ref().unwrap();
        assert_eq!(metadata["hook_type"], serde_json::json!(["TEST"]));
        assert_eq!(metadata["hook_count"], serde_json::json!(1));
        assert_eq!(metadata["priorities"], serde_json::json!([5]));
        assert_eq!(metadata["payloads"], serde_json::json!([{"k": "world"}]));
        let handlers = metadata["handlers"].as_array().unwrap();
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0]["name"], serde_json::json!("r1"));
        assert_eq!(handlers[0]["outcome"], serde_json::json!("continue"));
        assert!(metadata["duration_ms"].is_number());
        assert_eq!(
            metadata["handler_errors"].as_array().unwrap().len(),
            0,
            "no handler errors when all handlers resolve"
        );
    }

    #[tokio::test]
    async fn no_audit_event_without_hooks_or_receivers() {
        let registry = HookHandlerRegistry::new();
        let bus = Arc::new(EventBus::new(16));
        let mut sub = bus.subscribe();
        fire(
            &registry,
            &[],
            "UNCONFIGURED",
            &ctx_of("UNCONFIGURED"),
            Some(&bus),
        )
        .await;
        assert!(
            sub.try_recv().is_err(),
            "no event when nothing matched and no handler registered"
        );
    }

    #[tokio::test]
    async fn trigger_closed_fire_without_handlers_skips_audit() {
        let registry = HookHandlerRegistry::new();
        let bus = Arc::new(EventBus::new(16));
        let mut sub = bus.subscribe();
        let hooks = vec![hook_def("BEFORE_EXECUTE", 1, None)];
        let ctx = HookContext {
            execution_id: Id::from("exec-1".to_string()),
            hook_type: "BEFORE_EXECUTE".to_string(),
            data: HashMap::new(),
            cancellation: tokio_util::sync::CancellationToken::new(),
        };
        let summary = fire(&registry, &hooks, "BEFORE_EXECUTE", &ctx, Some(&bus)).await;
        assert_eq!(summary.handler_results.len(), 0);
        assert!(
            sub.try_recv().is_err(),
            "trigger-closed fire with no notified handler publishes no audit event"
        );
    }

    #[tokio::test]
    async fn gate_detail_is_none_at_non_gate_points() {
        let registry = HookHandlerRegistry::new();
        struct VetoHandler;
        #[async_trait::async_trait]
        impl HookHandler for VetoHandler {
            fn name(&self) -> &str {
                "observer"
            }
            async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
                HookOutcome::Veto {
                    reason: "observed denial".to_string(),
                }
            }
        }
        registry.register("AFTER_TOOL_CALL", Arc::new(VetoHandler), 1);
        let summary = fire(
            &registry,
            &[],
            "AFTER_TOOL_CALL",
            &ctx_of("AFTER_TOOL_CALL"),
            None,
        )
        .await;
        assert!(summary.outcome.is_veto());
        assert_eq!(
            summary.gate_rejection_detail(),
            None,
            "non-gate veto is observed, never a gate rejection"
        );
    }

    #[tokio::test]
    async fn gate_detail_and_reasons_stay_structured() {
        let registry = HookHandlerRegistry::new();
        struct FirstVeto;
        struct SecondVeto;
        #[async_trait::async_trait]
        impl HookHandler for FirstVeto {
            fn name(&self) -> &str {
                "first"
            }
            async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
                HookOutcome::Veto {
                    reason: "first denial".to_string(),
                }
            }
        }
        #[async_trait::async_trait]
        impl HookHandler for SecondVeto {
            fn name(&self) -> &str {
                "second"
            }
            async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
                HookOutcome::Veto {
                    reason: "second denial".to_string(),
                }
            }
        }
        registry.register("BEFORE_TOOL_CALL", Arc::new(FirstVeto), 1);
        registry.register("BEFORE_TOOL_CALL", Arc::new(SecondVeto), 1);

        let summary = fire(
            &registry,
            &[],
            "BEFORE_TOOL_CALL",
            &ctx_of("BEFORE_TOOL_CALL"),
            None,
        )
        .await;
        assert_eq!(
            summary.vetoed_reasons(),
            vec!["first denial", "second denial"]
        );
        assert_eq!(summary.vetoed_reason(), Some("first denial; second denial"));
        assert_eq!(
            summary.gate_rejection_detail(),
            Some("hook veto at BEFORE_TOOL_CALL: first denial; second denial".to_string())
        );

        let workflow_summary = fire(
            &registry,
            &[],
            "BEFORE_EXECUTE",
            &ctx_of("BEFORE_EXECUTE"),
            None,
        )
        .await;
        assert_eq!(workflow_summary.gate_rejection_detail(), None);
        assert!(summary
            .gate_rejection_detail()
            .is_some_and(|detail| { detail.starts_with("hook veto at BEFORE_TOOL_CALL: ") }));
    }

    #[tokio::test]
    async fn both_gate_points_share_one_rejection_wording() {
        let registry = HookHandlerRegistry::new();
        struct Deny;
        #[async_trait::async_trait]
        impl HookHandler for Deny {
            fn name(&self) -> &str {
                "deny"
            }
            async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
                HookOutcome::Veto {
                    reason: "denied".to_string(),
                }
            }
        }
        registry.register("BEFORE_TOOL_CALL", Arc::new(Deny), 1);
        let tool_summary = fire(
            &registry,
            &[],
            "BEFORE_TOOL_CALL",
            &ctx_of("BEFORE_TOOL_CALL"),
            None,
        )
        .await;
        assert_eq!(
            tool_summary.gate_rejection_detail(),
            Some("hook veto at BEFORE_TOOL_CALL: denied".to_string())
        );

        let registry = HookHandlerRegistry::new();
        registry.register("BEFORE_EXECUTE", Arc::new(Deny), 1);
        let exec_ctx = HookContext {
            execution_id: Id::from("exec-1".to_string()),
            hook_type: "BEFORE_EXECUTE".to_string(),
            data: HashMap::new(),
            cancellation: tokio_util::sync::CancellationToken::new(),
        };
        let exec_summary = fire(&registry, &[], "BEFORE_EXECUTE", &exec_ctx, None).await;
        assert_eq!(
            exec_summary.gate_rejection_detail(),
            Some("hook veto at BEFORE_EXECUTE: denied".to_string())
        );
    }

    #[tokio::test]
    async fn notification_uses_global_priority_order_with_stable_ties() {
        let registry = HookHandlerRegistry::new();
        registry.register(
            "TEST",
            Arc::new(CounterHandler {
                name: "dyn-high",
                calls: Arc::new(AtomicU32::new(0)),
                outcome: HookOutcome::Continue,
            }),
            10,
        );
        registry.register(
            "OTHER",
            Arc::new(CounterHandler {
                name: "static-low-target",
                calls: Arc::new(AtomicU32::new(0)),
                outcome: HookOutcome::Continue,
            }),
            0,
        );
        let hooks = vec![hook_def("TEST", 1, Some("static-low-target"))];
        let summary = fire(&registry, &hooks, "TEST", &ctx(), None).await;
        assert_eq!(summary.handler_results.len(), 2);
        assert_eq!(summary.handler_results[0].name, "dyn-high");
        assert_eq!(summary.handler_results[1].name, "static-low-target");

        let registry = HookHandlerRegistry::new();
        let static_calls = Arc::new(AtomicU32::new(0));
        let dynamic_calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "OTHER",
            Arc::new(CounterHandler {
                name: "static-target",
                calls: static_calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            0,
        );
        registry.register(
            "TIE",
            Arc::new(CounterHandler {
                name: "dyn-tie",
                calls: dynamic_calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            5,
        );
        let tie_ctx = HookContext {
            execution_id: Id::from("exec-1".to_string()),
            hook_type: "TIE".to_string(),
            data: HashMap::new(),
            cancellation: tokio_util::sync::CancellationToken::new(),
        };
        let hooks = vec![hook_def("TIE", 5, Some("static-target"))];
        let summary = fire(&registry, &hooks, "TIE", &tie_ctx, None).await;
        assert_eq!(summary.handler_results.len(), 2);
        assert_eq!(summary.handler_results[0].name, "static-target");
        assert_eq!(summary.handler_results[1].name, "dyn-tie");
    }

    #[tokio::test]
    async fn matched_ids_cover_condition_passed_definitions_only() {
        let registry = HookHandlerRegistry::new();
        let passed = HookDefinition {
            id: Id::from("h-passed".to_string()),
            hook_type: "TEST".to_string(),
            priority: 1,
            condition: None,
            enabled: true,
            payload: None,
            handler: None,
            create_checkpoint: Some(true),
            checkpoint_description: None,
        };
        let filtered = HookDefinition {
            id: Id::from("h-filtered".to_string()),
            hook_type: "TEST".to_string(),
            priority: 1,
            condition: Some("missing_flag".to_string()),
            enabled: true,
            payload: None,
            handler: None,
            create_checkpoint: Some(true),
            checkpoint_description: None,
        };
        let summary = fire(&registry, &[passed, filtered], "TEST", &ctx(), None).await;
        assert_eq!(summary.matched_hook_ids, vec!["h-passed".to_string()]);
    }
}
