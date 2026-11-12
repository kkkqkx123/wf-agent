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
        attachment: None,
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
        attachment: None,
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
        attachment: None,
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
        attachment: None,
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
        attachment: None,
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
        attachment: None,
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

fn summary_text(message: &Message) -> &str {
    match &message.content {
        wf_types::message::MessageContentValue::Text(text) => text,
        _ => panic!("test summaries are plain text"),
    }
}

struct ReplaceHandler {
    name: &'static str,
    text: &'static str,
}

#[async_trait::async_trait]
impl HookHandler for ReplaceHandler {
    fn name(&self) -> &str {
        self.name
    }
    async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
        HookOutcome::Replace {
            summaries: vec![wf_llm::message_builder::user_text(self.text)],
        }
    }
}

#[tokio::test]
async fn replace_first_in_notification_order_wins() {
    let registry = HookHandlerRegistry::new();
    registry.register(
        "TEST",
        Arc::new(ReplaceHandler {
            name: "low",
            text: "low",
        }),
        1,
    );
    registry.register(
        "TEST",
        Arc::new(ReplaceHandler {
            name: "high",
            text: "high",
        }),
        10,
    );

    let summary = fire(&registry, &[], "TEST", &ctx(), None).await;
    let replacement = summary.replacement.expect("replace must be captured");
    assert_eq!(replacement.len(), 1);
    assert_eq!(summary_text(&replacement[0]), "high");
    assert!(matches!(summary.outcome, HookOutcome::Replace { .. }));
}

#[tokio::test]
async fn veto_clears_replacement() {
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
    registry.register(
        "TEST",
        Arc::new(ReplaceHandler {
            name: "repl",
            text: "x",
        }),
        1,
    );
    registry.register("TEST", Arc::new(Deny), 2);

    let summary = fire(&registry, &[], "TEST", &ctx(), None).await;
    assert!(summary.outcome.is_veto());
    assert!(summary.replacement.is_none());
}

#[tokio::test]
async fn named_takeover_skips_fire_but_resolves_by_name() {
    let registry = HookHandlerRegistry::new();
    let calls = Arc::new(AtomicU32::new(0));
    assert!(registry.register_named(Arc::new(CounterHandler {
        name: "takeover",
        calls: calls.clone(),
        outcome: HookOutcome::Continue,
    })));
    assert!(registry.contains("takeover"));
    assert!(registry.for_type("TEST").is_empty());

    let summary = fire(&registry, &[], "TEST", &ctx(), None).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "named-only handlers are never auto-notified"
    );
    assert!(summary.handler_results.is_empty());

    let resolved = registry.get("takeover").expect("takeover resolves by name");
    let registered = crate::hooks::registry::RegisteredHandler {
        name: resolved.name().to_string(),
        priority: 0,
        handler: resolved,
        condition: None,
    };
    registry.notify(&ctx(), &registered).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
