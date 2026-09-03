//! Unified hook dispatch: evaluation → payload resolution → ordered
//! notification → outcome aggregation → audit publication.
//!
//! The engine calls [`dispatch`] at a hook point and awaits it: the
//! notification barrier completes before the engine moves on. The pipeline
//! itself carries no behavior — filtering (condition / enabled / weight),
//! payload resolution and ordered notification only; behavior lives in
//! registered [`HookReceiver`]s.

use serde_json::Value;
use tracing::warn;
use wf_core::EventBus;

use crate::hooks::emit::{
    evaluate_hook_condition, filter_and_sort_hooks, publish_hook_audit_event,
};
use crate::hooks::registry::HookRegistry;
use crate::hooks::template::resolve_payload_template;
use crate::hooks::types::{BaseHookDefinition, HookContext, HookOutcome};

/// Outcome of one receiver notification.
#[derive(Debug, Clone)]
pub struct ReceiverResult {
    pub name: String,
    pub outcome: HookOutcome,
    pub duration_ms: i64,
    /// Timeout / unresolvable receiver description; `None` on success.
    pub error: Option<String>,
}

/// Aggregate result of one dispatch: everything the audit trail needs
/// (payloads, per-receiver results, duration) plus the aggregated outcome.
#[derive(Debug, Clone)]
pub struct DispatchSummary {
    pub hook_type: String,
    pub payloads: Vec<Value>,
    pub weights: Vec<i32>,
    pub receiver_results: Vec<ReceiverResult>,
    pub duration_ms: i64,
    pub outcome: HookOutcome,
}

/// Dispatch a hook point:
///
/// 1. statically evaluate the hook definitions of `hook_type`
///    (condition / enabled / weight filtering) and resolve payload templates;
/// 2. synchronously notify every receiver that passes evaluation — the
///    `receiver`-named receivers of the static definitions first, then the
///    receivers dynamically registered on the hook type (weight descending);
/// 3. aggregate the outcomes (first `Intercept` wins);
/// 4. publish the `HOOK_TRIGGERED` audit event carrying the payloads and the
///    per-receiver results.
///
/// The notification barrier is awaited by the caller: dispatch returns only
/// after every receiver settled (each guarded by the registry timeout).
pub async fn dispatch(
    registry: &HookRegistry,
    hooks: &[BaseHookDefinition],
    hook_type: &str,
    ctx: &HookContext,
    event_bus: Option<&EventBus>,
) -> DispatchSummary {
    let started = wf_common::now();

    let mut payloads: Vec<Value> = Vec::new();
    let mut weights: Vec<i32> = Vec::new();
    let mut matched: Vec<BaseHookDefinition> = Vec::new();
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

    // Static definitions with an explicit receiver name are notified in
    // weight order; unresolvable names are reported, never fatal.
    let mut receiver_results: Vec<ReceiverResult> = Vec::new();
    for def in &matched {
        let Some(name) = def.receiver.as_deref() else {
            continue;
        };
        match registry.get(name) {
            Some(receiver) => {
                let registered = crate::hooks::registry::RegisteredReceiver {
                    name: name.to_string(),
                    weight: def.weight,
                    receiver,
                };
                receiver_results.push(registry.notify(ctx, &registered).await);
            }
            None => {
                warn!(
                    hook_id = %def.id,
                    receiver = %name,
                    "hook receiver '{}' is not registered, skipping",
                    name
                );
                receiver_results.push(ReceiverResult {
                    name: name.to_string(),
                    outcome: HookOutcome::Continue,
                    duration_ms: 0,
                    error: Some("receiver not registered".to_string()),
                });
            }
        }
    }

    // Dynamically registered receivers for the hook type, weight descending.
    for registered in registry.for_type(hook_type) {
        receiver_results.push(registry.notify(ctx, &registered).await);
    }

    let duration_ms = wf_common::now() - started;
    let outcome = aggregate_outcome(&receiver_results);

    publish_hook_audit_event(
        event_bus,
        ctx,
        &payloads,
        &weights,
        &receiver_results,
        duration_ms,
    );

    DispatchSummary {
        hook_type: hook_type.to_string(),
        payloads,
        weights,
        receiver_results,
        duration_ms,
        outcome,
    }
}

/// First `Intercept` wins; otherwise `Continue`.
fn aggregate_outcome(results: &[ReceiverResult]) -> HookOutcome {
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
    use crate::hooks::receiver::HookReceiver;
    use wf_types::Id;

    struct CounterReceiver {
        name: &'static str,
        calls: Arc<AtomicU32>,
        outcome: HookOutcome,
    }

    #[async_trait::async_trait]
    impl HookReceiver for CounterReceiver {
        fn name(&self) -> &str {
            self.name
        }
        async fn on_hook(&self, ctx: &HookContext) -> HookOutcome {
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

    fn hook_def(hook_type: &str, weight: i32, receiver: Option<&str>) -> BaseHookDefinition {
        BaseHookDefinition {
            id: Id::new(),
            hook_type: hook_type.to_string(),
            weight,
            condition: None,
            enabled: true,
            payload: None,
            receiver: receiver.map(String::from),
        }
    }

    #[tokio::test]
    async fn registered_receiver_is_notified_synchronously() {
        let registry = HookRegistry::new();
        let calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "TEST",
            Arc::new(CounterReceiver {
                name: "r1",
                calls: calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
        );

        let summary = dispatch(&registry, &[], "TEST", &ctx(), None).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(summary.receiver_results.len(), 1);
        assert_eq!(summary.receiver_results[0].name, "r1");
        assert_eq!(summary.outcome, HookOutcome::Continue);
    }

    #[tokio::test]
    async fn receiver_field_resolves_named_receiver() {
        let registry = HookRegistry::new();
        let calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "OTHER",
            Arc::new(CounterReceiver {
                name: "named",
                calls: calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
        );

        let hooks = vec![hook_def("TEST", 1, Some("named"))];
        let summary = dispatch(&registry, &hooks, "TEST", &ctx(), None).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(summary.receiver_results.len(), 1);
        assert_eq!(summary.receiver_results[0].name, "named");
    }

    #[tokio::test]
    async fn unresolvable_receiver_is_reported_not_fatal() {
        let registry = HookRegistry::new();
        let hooks = vec![hook_def("TEST", 1, Some("missing"))];
        let summary = dispatch(&registry, &hooks, "TEST", &ctx(), None).await;
        assert_eq!(summary.receiver_results.len(), 1);
        assert_eq!(summary.receiver_results[0].name, "missing");
        assert!(summary.receiver_results[0].error.is_some());
        assert_eq!(summary.outcome, HookOutcome::Continue);
    }

    #[tokio::test]
    async fn condition_filters_definition_before_notification() {
        let registry = HookRegistry::new();
        let calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "TEST",
            Arc::new(CounterReceiver {
                name: "r1",
                calls: calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
        );

        // Dynamic receivers always run; the static definition is filtered.
        let hooks = vec![BaseHookDefinition {
            id: Id::new(),
            hook_type: "TEST".to_string(),
            weight: 1,
            condition: Some("missing_flag".to_string()),
            enabled: true,
            payload: None,
            receiver: Some("r1".to_string()),
        }];
        let summary = dispatch(&registry, &hooks, "TEST", &ctx(), None).await;
        assert_eq!(
            summary.receiver_results.len(),
            1,
            "only the dynamic receiver runs"
        );
        assert_eq!(summary.receiver_results[0].name, "r1");
    }

    #[tokio::test]
    async fn intercept_outcome_wins_aggregation() {
        let registry = HookRegistry::new();
        let continue_calls = Arc::new(AtomicU32::new(0));
        let intercept_calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "TEST",
            Arc::new(CounterReceiver {
                name: "continue-r",
                calls: continue_calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
        );
        registry.register(
            "TEST",
            Arc::new(CounterReceiver {
                name: "intercept-r",
                calls: intercept_calls.clone(),
                outcome: HookOutcome::Intercept {
                    reason: "blocked".to_string(),
                },
            }),
            10,
        );

        let summary = dispatch(&registry, &[], "TEST", &ctx(), None).await;
        assert_eq!(
            summary.outcome,
            HookOutcome::Intercept {
                reason: "blocked".to_string()
            }
        );
        // Notified in weight order.
        assert_eq!(summary.receiver_results[0].name, "intercept-r");
        assert_eq!(summary.receiver_results[1].name, "continue-r");
    }

    #[tokio::test]
    async fn timeout_receiver_does_not_block_engine() {
        let registry = HookRegistry::new().with_timeout(std::time::Duration::from_millis(20));

        struct SlowReceiver;
        #[async_trait::async_trait]
        impl HookReceiver for SlowReceiver {
            fn name(&self) -> &str {
                "slow"
            }
            async fn on_hook(&self, _ctx: &HookContext) -> HookOutcome {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                HookOutcome::Continue
            }
        }

        registry.register("TEST", Arc::new(SlowReceiver), 1);
        let summary = dispatch(&registry, &[], "TEST", &ctx(), None).await;
        assert_eq!(summary.receiver_results.len(), 1);
        assert!(summary.receiver_results[0].error.is_some());
        assert_eq!(summary.outcome, HookOutcome::Continue);
    }

    #[tokio::test]
    async fn audit_event_published_with_results_summary() {
        use wf_core::EventBus;
        use wf_types::events::EventType;

        let registry = HookRegistry::new();
        let calls = Arc::new(AtomicU32::new(0));
        registry.register(
            "TEST",
            Arc::new(CounterReceiver {
                name: "r1",
                calls: calls.clone(),
                outcome: HookOutcome::Continue,
            }),
            1,
        );

        let bus = Arc::new(EventBus::new(16));
        let mut sub = bus.subscribe();

        let hooks = vec![BaseHookDefinition {
            id: Id::new(),
            hook_type: "TEST".to_string(),
            weight: 5,
            condition: None,
            enabled: true,
            payload: Some(serde_json::json!({"k": "{{name}}"})),
            receiver: None,
        }];
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("world".to_string()));
        let ctx = HookContext {
            execution_id: Id::from("exec-1".to_string()),
            hook_type: "TEST".to_string(),
            data,
        };

        dispatch(&registry, &hooks, "TEST", &ctx, Some(&bus)).await;

        let event = sub.try_recv().expect("audit event must be published");
        assert_eq!(event.r#type, EventType::HookTriggered);
        assert_eq!(event.execution_id.as_deref(), Some("exec-1"));
        let metadata = event.metadata.as_ref().unwrap();
        assert_eq!(metadata["hook_type"], serde_json::json!(["TEST"]));
        assert_eq!(metadata["hook_count"], serde_json::json!(1));
        assert_eq!(metadata["weights"], serde_json::json!([5]));
        assert_eq!(metadata["payloads"], serde_json::json!([{"k": "world"}]));
        let receivers = metadata["receivers"].as_array().unwrap();
        assert_eq!(receivers.len(), 1);
        assert_eq!(receivers[0]["name"], serde_json::json!("r1"));
        assert_eq!(receivers[0]["outcome"], serde_json::json!("continue"));
        assert!(metadata["duration_ms"].is_number());
        assert_eq!(
            metadata["receiver_errors"].as_array().unwrap().len(),
            0,
            "no receiver errors when all receivers resolve"
        );
    }

    #[tokio::test]
    async fn no_audit_event_without_hooks_or_receivers() {
        let registry = HookRegistry::new();
        let bus = Arc::new(EventBus::new(16));
        let mut sub = bus.subscribe();
        dispatch(&registry, &[], "UNCONFIGURED", &ctx(), Some(&bus)).await;
        assert!(
            sub.try_recv().is_err(),
            "no event when nothing matched and no receiver registered"
        );
    }
}
