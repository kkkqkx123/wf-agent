//! Unified hook fire: evaluation → payload resolution → ordered
//! notification → outcome aggregation → audit publication.
//!
//! The engine calls [`fire`] at a hook point and awaits it: the
//! notification barrier completes before the engine moves on. The pipeline
//! itself carries no behavior — filtering (condition / enabled / weight),
//! payload resolution and ordered notification only; behavior lives in
//! registered [`HookHandler`]s.

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
    pub outcome: HookOutcome,
    pub duration_ms: i64,
    /// Timeout / unresolvable handler description; `None` on success.
    pub error: Option<String>,
}

/// Aggregate result of one fire: everything the audit trail needs
/// (payloads, per-handler results, duration) plus the aggregated outcome.
#[derive(Debug, Clone)]
pub struct FireSummary {
    pub hook_type: String,
    pub payloads: Vec<Value>,
    pub weights: Vec<i32>,
    pub handler_results: Vec<HandlerResult>,
    pub duration_ms: i64,
    pub outcome: HookOutcome,
}

/// Fire a hook point:
///
/// 1. statically evaluate the hook definitions of `hook_type`
///    (condition / enabled / weight filtering) and resolve payload templates;
/// 2. synchronously notify every handler that passes evaluation — the
///    `handler`-named handlers of the static definitions first, then the
///    handlers dynamically registered on the hook type (weight descending);
/// 3. aggregate the outcomes (first `Intercept` wins);
/// 4. publish the `HOOK_TRIGGERED` audit event carrying the payloads and the
///    per-handler results.
///
/// The notification barrier is awaited by the caller: fire returns only
/// after every handler settled (each guarded by the registry timeout).
pub async fn fire(
    registry: &HookHandlerRegistry,
    hooks: &[HookDefinition],
    hook_type: &str,
    ctx: &HookContext,
    event_bus: Option<&EventBus>,
) -> FireSummary {
    let started = wf_common::now();

    let mut payloads: Vec<Value> = Vec::new();
    let mut weights: Vec<i32> = Vec::new();
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
        weights.push(hook.weight);
        matched.push(hook);
    }

    // Static definitions with an explicit handler name are notified in
    // weight order; unresolvable names are reported, never fatal.
    let mut handler_results: Vec<HandlerResult> = Vec::new();
    for def in &matched {
        let Some(name) = def.handler.as_deref() else {
            continue;
        };
        match registry.get(name) {
            Some(handler) => {
                let registered = crate::hooks::registry::RegisteredHandler {
                    name: name.to_string(),
                    weight: def.weight,
                    handler,
                };
                handler_results.push(registry.notify(ctx, &registered).await);
            }
            None => {
                warn!(
                    hook_id = %def.id,
                    handler = %name,
                    "hook handler '{}' is not registered, skipping",
                    name
                );
                handler_results.push(HandlerResult {
                    name: name.to_string(),
                    outcome: HookOutcome::Continue,
                    duration_ms: 0,
                    error: Some("handler not registered".to_string()),
                });
            }
        }
    }

    // Dynamically registered handlers for the hook type, weight descending.
    for registered in registry.for_type(hook_type) {
        handler_results.push(registry.notify(ctx, &registered).await);
    }

    let duration_ms = wf_common::now() - started;
    let outcome = aggregate_outcome(&handler_results);

    publish_hook_audit_event(
        event_bus,
        ctx,
        &payloads,
        &weights,
        &handler_results,
        duration_ms,
    );

    FireSummary {
        hook_type: hook_type.to_string(),
        payloads,
        weights,
        handler_results,
        duration_ms,
        outcome,
    }
}

/// First `Intercept` wins; otherwise `Continue`.
fn aggregate_outcome(results: &[HandlerResult]) -> HookOutcome {
    for result in results {
        if let HookOutcome::Intercept { reason } = &result.outcome {
            return HookOutcome::Intercept {
                reason: reason.clone(),
            };
        }
    }
    HookOutcome::Continue
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
        HookContext {
            execution_id: Id::from("exec-1".to_string()),
            hook_type: "TEST".to_string(),
            data: HashMap::new(),
        }
    }

    fn hook_def(hook_type: &str, weight: i32, handler: Option<&str>) -> HookDefinition {
        HookDefinition {
            id: Id::new(),
            hook_type: hook_type.to_string(),
            weight,
            condition: None,
            enabled: true,
            payload: None,
            handler: handler.map(String::from),
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
            weight: 1,
            condition: Some("missing_flag".to_string()),
            enabled: true,
            payload: None,
            handler: Some("r1".to_string()),
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
    async fn intercept_outcome_wins_aggregation() {
        let registry = HookHandlerRegistry::new();
        let continue_calls = Arc::new(AtomicU32::new(0));
        let intercept_calls = Arc::new(AtomicU32::new(0));
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
                name: "intercept-r",
                calls: intercept_calls.clone(),
                outcome: HookOutcome::Intercept {
                    reason: "blocked".to_string(),
                },
            }),
            10,
        );

        let summary = fire(&registry, &[], "TEST", &ctx(), None).await;
        assert_eq!(
            summary.outcome,
            HookOutcome::Intercept {
                reason: "blocked".to_string()
            }
        );
        // Notified in weight order.
        assert_eq!(summary.handler_results[0].name, "intercept-r");
        assert_eq!(summary.handler_results[1].name, "continue-r");
    }

    #[tokio::test]
    async fn timeout_handler_does_not_block_engine() {
        let registry =
            HookHandlerRegistry::new().with_timeout(std::time::Duration::from_millis(20));

        struct SlowHandler;
        #[async_trait::async_trait]
        impl HookHandler for SlowHandler {
            fn name(&self) -> &str {
                "slow"
            }
            async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                HookOutcome::Continue
            }
        }

        registry.register("TEST", Arc::new(SlowHandler), 1);
        let summary = fire(&registry, &[], "TEST", &ctx(), None).await;
        assert_eq!(summary.handler_results.len(), 1);
        assert!(summary.handler_results[0].error.is_some());
        assert_eq!(summary.outcome, HookOutcome::Continue);
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
            weight: 5,
            condition: None,
            enabled: true,
            payload: Some(serde_json::json!({"k": "{{name}}"})),
            handler: None,
        }];
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("world".to_string()));
        let ctx = HookContext {
            execution_id: Id::from("exec-1".to_string()),
            hook_type: "TEST".to_string(),
            data,
        };

        fire(&registry, &hooks, "TEST", &ctx, Some(&bus)).await;

        let event = sub.try_recv().expect("audit event must be published");
        assert_eq!(event.r#type, EventType::HookTriggered);
        assert_eq!(event.execution_id.as_deref(), Some("exec-1"));
        let metadata = event.metadata.as_ref().unwrap();
        assert_eq!(metadata["hook_type"], serde_json::json!(["TEST"]));
        assert_eq!(metadata["hook_count"], serde_json::json!(1));
        assert_eq!(metadata["weights"], serde_json::json!([5]));
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
        fire(&registry, &[], "UNCONFIGURED", &ctx(), Some(&bus)).await;
        assert!(
            sub.try_recv().is_err(),
            "no event when nothing matched and no handler registered"
        );
    }
}
