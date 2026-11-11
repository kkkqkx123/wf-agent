use super::*;

#[tokio::test]
async fn compression_fire_takes_over_immediately() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());
    wf_resource::predefined::workflow::register(&registries, &RegisterOptions::default());

    // The stub summary runner: records the takeover and then blocks far
    // beyond the fire timeout — the engine must not wait for it.
    struct StuckRunner(Arc<AtomicBool>);
    #[async_trait]
    impl SubworkflowRunner for StuckRunner {
        async fn run(&self, _workflow_id: &str, _input: Value) -> WorkflowResult<Value> {
            self.0.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(Value::Null)
        }
    }

    let contexts = Arc::new(ExecutionContextRegistry::new());
    let hook_handler_registry = Arc::new(HookHandlerRegistry::new());
    let started = Arc::new(AtomicBool::new(false));
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(StuckRunner(started.clone()));
    register_compression_handler(
        &hook_handler_registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
        },
    );
    let listener = start_routed_compression_listener(
        &bus,
        &contexts,
        runner,
        wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID,
        None,
        CompressionPolicy::default(),
    )
    .await;

    // A valid compression request (message snapshot present).
    let messages: Vec<Message> = vec![text_message(MessageRole::User, "long message")];
    let request = wf_execution_shared::context_store::compression_request(
        "chat",
        900,
        1000,
        messages.len(),
        1,
        false,
        &messages,
        &[],
        "",
    );

    // Dispatch returns as soon as the routed handoff is published,
    // never after it completes (the stub blocks 10s).
    let elapsed = std::time::Instant::now();
    let outcome = wf_execution_shared::context_store::dispatch_compression_signal(
        Some(hook_handler_registry.as_ref()),
        Some(bus.as_ref()),
        &Id::from("wf-run".to_string()),
        None,
        tokio_util::sync::CancellationToken::new(),
        &request,
        &[],
    )
    .await;
    assert_eq!(
        outcome,
        wf_execution_shared::CompressionSignalOutcome::Taken
    );
    let fire_ms = elapsed.elapsed().as_millis();
    assert!(
        fire_ms < 2000,
        "dispatch must return at takeover, not after compression (took {fire_ms}ms)"
    );
    // The routed summary task is observable as soon as the listener claims
    // the handoff: fire did not await it (it is still blocked in
    // the stub for the rest of the test).
    wait_until(|| started.load(Ordering::SeqCst)).await;
    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn run_routed_rejects_non_handoff_without_spawn() {
    use wf_execution_shared::{
        build_context_compression_requested_event, ContextCompressionRequest,
    };
    let bus = Arc::new(EventBus::new(64));
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let runner = Arc::new(FlakySummaryRunner {
        fail_first: 0,
        calls: std::sync::atomic::AtomicU32::new(0),
    });
    let pipeline = super::super::compression::CompressionPipeline::with_ledger(
        bus,
        runner.clone(),
        contexts,
        "stub/llm-summary".to_string(),
        CancellationToken::new(),
        None,
    );
    let messages = vec![text_message(MessageRole::User, "long message")];
    let audit = build_context_compression_requested_event(
        "exec-1",
        Some("loop-1"),
        &ContextCompressionRequest {
            target_context_id: "chat",
            tokens_used: 900,
            token_limit: 1000,
            message_count: messages.len(),
            array_version: 7,
            forced: false,
            messages: &messages,
            plan: &[],
            directive: "",
        },
    );
    let template = super::super::compression::builtin_compression_template();
    assert_eq!(
        template.name,
        super::super::compression::BUILTIN_COMPRESSION_TEMPLATE_NAME
    );
    // The emitter's audit copy carries no snapshot and never matches the
    // builtin condition; reaching the route anyway skips without spawn.
    pipeline
        .run_routed(&template, &audit)
        .await
        .expect("audit skip is clean");
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    // A non-builtin action on the route fails loudly instead of running.
    let mut wrong = template.clone();
    wrong.action = Some(TriggerAction::StopWorkflowExecution {});
    assert!(pipeline.run_routed(&wrong, &audit).await.is_err());
    // Execution-less handoffs fail loudly (the pipeline needs the
    // emitting execution for write-back identity).
    let mut no_exec = audit.clone();
    no_exec.execution_id = None;
    assert!(pipeline.run_routed(&template, &no_exec).await.is_err());
}

#[tokio::test]
async fn routed_compression_end_to_end_via_listener() {
    // Full routed flow: hook fire -> adapter publishes the handoff ->
    // listener matches the builtin template -> router claims through the
    // pipeline -> terminal event. The adapter never spawns directly, so
    // every chain observed here traversed template matching.
    let bus = Arc::new(EventBus::new(256));
    let mut sub = bus.subscribe();
    let registries = Arc::new(ResourceRegistries::new());
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let gateway = Arc::new(LlmGateway::new());
    let hook_handler_registry = Arc::new(HookHandlerRegistry::new());
    let runner = Arc::new(FlakySummaryRunner {
        fail_first: 0,
        calls: std::sync::atomic::AtomicU32::new(0),
    });
    let policy =
        stub_compression_policy(0, 5_000, wf_types::workflow::CompressionFallbackMode::Fail);
    let listener = start_trigger_listener_with_parts(ListenerDeps {
        event_bus: bus.clone(),
        registries: registries.clone(),
        contexts: contexts.clone(),
        runner: runner.clone(),
        gateway,
        tool_registry: None,
        sandbox: None,
        agent_executor: None,
        agent_loop_registry: None,
        ledger: None,
        hook_handler_registry: None,
        signal_bus: None,
        timer_bindings: None,
        schedule_state_store: None,
        shutdown: CancellationToken::new(),
        compression_route: Some(CompressionRouteConfig {
            summary_workflow_id: "stub/llm-summary".to_string(),
            repress_workflow_id: None,
            policy: policy.clone(),
        }),
    });
    register_compression_handler(
        &hook_handler_registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
        },
    );
    wait_for_listener(&bus, 1).await;

    let messages = vec![text_message(MessageRole::User, "long message")];
    fire_compression_signal(
        &hook_handler_registry,
        &bus,
        &agent_compression_signal(&messages),
    )
    .await;
    next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 1);

    // The claim released with the terminal event: re-firing the same
    // version runs exactly one more chain through the route.
    fire_compression_signal(
        &hook_handler_registry,
        &bus,
        &agent_compression_signal(&messages),
    )
    .await;
    next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 2);

    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn compression_retry_recovers_before_terminal_failure() {
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let registry = Arc::new(HookHandlerRegistry::new());
    let runner = Arc::new(FlakySummaryRunner {
        fail_first: 1,
        calls: std::sync::atomic::AtomicU32::new(0),
    });
    let policy =
        stub_compression_policy(1, 5_000, wf_types::workflow::CompressionFallbackMode::Fail);
    register_compression_handler(
        &registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
        },
    );
    let listener = start_routed_compression_listener(
        &bus,
        &contexts,
        runner.clone(),
        "stub/llm-summary",
        None,
        policy,
    )
    .await;

    let messages = vec![text_message(MessageRole::User, "long message")];
    fire_compression_signal(&registry, &bus, &agent_compression_signal(&messages)).await;

    next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    // First attempt failed, second succeeded: no FAILED event landed.
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn compression_attempt_timeout_is_bounded_and_retried() {
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let registry = Arc::new(HookHandlerRegistry::new());
    let runner = Arc::new(HangingFirstRunner {
        calls: std::sync::atomic::AtomicU32::new(0),
    });
    // A hung attempt must be cut at 300ms, not stall the chain.
    let policy = stub_compression_policy(1, 300, wf_types::workflow::CompressionFallbackMode::Fail);
    register_compression_handler(
        &registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
        },
    );
    let listener = start_routed_compression_listener(
        &bus,
        &contexts,
        runner.clone(),
        "stub/llm-summary",
        None,
        policy,
    )
    .await;

    let messages = vec![text_message(MessageRole::User, "long message")];
    fire_compression_signal(&registry, &bus, &agent_compression_signal(&messages)).await;

    next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn compression_duplicate_signal_same_version_runs_once() {
    // Claim mechanics live on the listener route pipeline: a second
    // identical handoff while the first chain is still in flight loses the
    // claim race, so the stub runner observes exactly one chain (hung
    // attempt cut at the attempt timeout, then the retry success).
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let registry = Arc::new(HookHandlerRegistry::new());
    let runner = Arc::new(HangingFirstRunner {
        calls: std::sync::atomic::AtomicU32::new(0),
    });
    let policy = stub_compression_policy(1, 300, wf_types::workflow::CompressionFallbackMode::Fail);
    register_compression_handler(
        &registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
        },
    );
    let listener = start_routed_compression_listener(
        &bus,
        &contexts,
        runner.clone(),
        "stub/llm-summary",
        None,
        policy,
    )
    .await;

    let messages = vec![text_message(MessageRole::User, "long message")];
    let ctx = agent_compression_signal(&messages);
    fire_compression_signal(&registry, &bus, &ctx).await;
    fire_compression_signal(&registry, &bus, &ctx).await;

    next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn compression_terminal_failure_publishes_failed_and_releases_claim() {
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let registry = Arc::new(HookHandlerRegistry::new());
    // Every attempt fails: max_retries 1 means two runs then terminal.
    let runner = Arc::new(FlakySummaryRunner {
        fail_first: u32::MAX,
        calls: std::sync::atomic::AtomicU32::new(0),
    });
    // `fail` (the default mode): a terminal failure publishes
    // FAILED for external handling instead of landing a
    // degraded window.
    let policy =
        stub_compression_policy(1, 5_000, wf_types::workflow::CompressionFallbackMode::Fail);
    register_compression_handler(
        &registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
        },
    );
    let listener = start_routed_compression_listener(
        &bus,
        &contexts,
        runner.clone(),
        "stub/llm-summary",
        None,
        policy,
    )
    .await;

    let messages = vec![text_message(MessageRole::User, "long message")];
    let ctx = agent_compression_signal(&messages);
    fire_compression_signal(&registry, &bus, &ctx).await;
    let failed = next_compression_event(&mut sub, EventType::ContextCompressionFailed).await;
    let meta = wf_execution_shared::ContextCompressionFailedMeta::try_from(&failed).unwrap();
    assert_eq!(meta.target_context_id, "chat");
    assert_eq!(meta.array_version, 7);
    assert_eq!(meta.attempts, 2);

    // The RAII guard released the dedup claim at the terminal state: the
    // identical signal is taken over again instead of swallowed.
    fire_compression_signal(&registry, &bus, &ctx).await;
    next_compression_event(&mut sub, EventType::ContextCompressionFailed).await;
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 4);
    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn compression_terminal_failure_partial_summary_lands_visible_window() {
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let registry = Arc::new(HookHandlerRegistry::new());
    // Every attempt fails, but the declared `partial_summary` policy must
    // land a degraded COMPLETED so the emitting execution survives.
    let runner = Arc::new(FlakySummaryRunner {
        fail_first: u32::MAX,
        calls: std::sync::atomic::AtomicU32::new(0),
    });
    let policy = stub_compression_policy(
        0,
        5_000,
        wf_types::workflow::CompressionFallbackMode::PartialSummary,
    );
    register_compression_handler(
        &registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
        },
    );
    let listener = start_routed_compression_listener(
        &bus,
        &contexts,
        runner.clone(),
        "stub/llm-summary",
        None,
        policy,
    )
    .await;

    let messages = vec![
        text_message(MessageRole::User, "oldest long message"),
        text_message(MessageRole::User, "newest long message"),
    ];
    fire_compression_signal(&registry, &bus, &agent_compression_signal(&messages)).await;

    let completed = next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    let meta = wf_execution_shared::ContextCompressionCompletedMeta::try_from(&completed).unwrap();
    assert!(meta.degraded, "fallback completion must be marked degraded");
    // The trimmed window is non-empty (newest messages retained).
    assert!(!meta.messages.is_empty());
    // Truncation is never transparent to the LLM: the window is headed
    // by a system notice naming the failure.
    let head = &meta.messages[0];
    assert_eq!(head.role, MessageRole::System);
    let MessageContentValue::Text(text) = &head.content else {
        panic!("degraded window must start with a text notice");
    };
    assert!(
        text.contains("[context compression notice]"),
        "notice head missing: {text}"
    );
    stop_trigger_listener(listener).await;
}
