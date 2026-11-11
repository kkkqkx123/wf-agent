use super::*;

#[tokio::test]
async fn compression_plan_runs_per_item_and_writes_back_once() {
    // Plan mode over one pipeline: two items run the summary chain serially
    // and a single aggregated completion lands for the whole batch (the
    // version anchor advances once). An empty plan keeps the legacy
    // single-summary behavior; this test covers the non-empty path.
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let registry = Arc::new(HookHandlerRegistry::new());
    let runner = Arc::new(FlakySummaryRunner {
        fail_first: 0,
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

    let messages = vec![
        text_message(MessageRole::User, "first"),
        text_message(MessageRole::User, "second"),
        text_message(MessageRole::User, "third"),
        text_message(MessageRole::User, "fourth"),
    ];
    let mut ctx = agent_compression_signal(&messages);
    let plan = vec![
        wf_execution_shared::CompactionPlanItem {
            block_id: "b0001".to_string(),
            start: 0,
            end: 2,
            depth: 0,
            extra_block_ids: Vec::new(),
        },
        wf_execution_shared::CompactionPlanItem {
            block_id: "b0002".to_string(),
            start: 2,
            end: 99,
            depth: 0,
            extra_block_ids: Vec::new(),
        },
    ];
    ctx.data.insert(
        wf_execution_shared::KEY_PLAN.to_string(),
        serde_json::to_value(&plan).expect("plan serializes"),
    );
    fire_compression_signal(&registry, &bus, &ctx).await;
    let completed = next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    let meta = wf_execution_shared::ContextCompressionCompletedMeta::try_from(&completed).unwrap();
    assert_eq!(meta.target_context_id, "chat");
    assert_eq!(meta.array_version, 7);
    assert_eq!(meta.messages.len(), 2);
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    stop_trigger_listener(listener).await;
}

/// Stub summary runner recording which chain served each item.
struct RecordingChainRunner {
    calls: std::sync::atomic::AtomicU32,
    workflow_ids: std::sync::Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl SubworkflowRunner for RecordingChainRunner {
    async fn run(&self, workflow_id: &str, _input: Value) -> WorkflowResult<Value> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.workflow_ids
            .lock()
            .unwrap()
            .push(workflow_id.to_string());
        Ok(serde_json::to_value(vec![text_message(
            MessageRole::Assistant,
            "repressed summary",
        )])
        .expect("message array serializes"))
    }
}

#[tokio::test]
async fn recompression_items_run_the_repress_chain() {
    // A depth-1 plan item runs the repress chain variant while a depth-0
    // item stays on the main chain; one aggregated completion lands.
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let registry = Arc::new(HookHandlerRegistry::new());
    let runner = Arc::new(RecordingChainRunner {
        calls: std::sync::atomic::AtomicU32::new(0),
        workflow_ids: std::sync::Mutex::new(Vec::new()),
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
        Some("stub/llm-summary-repress".to_string()),
        policy,
    )
    .await;

    let messages = vec![
        text_message(MessageRole::User, "first"),
        text_message(MessageRole::User, "second"),
        text_message(MessageRole::User, "third"),
        text_message(MessageRole::User, "fourth"),
    ];
    let mut ctx = agent_compression_signal(&messages);
    let plan = vec![
        wf_execution_shared::CompactionPlanItem {
            block_id: "b0001".to_string(),
            start: 0,
            end: 2,
            depth: 0,
            extra_block_ids: Vec::new(),
        },
        wf_execution_shared::CompactionPlanItem {
            block_id: "b0002".to_string(),
            start: 2,
            end: 4,
            depth: 1,
            extra_block_ids: Vec::new(),
        },
    ];
    ctx.data.insert(
        wf_execution_shared::KEY_PLAN.to_string(),
        serde_json::to_value(&plan).expect("plan serializes"),
    );
    fire_compression_signal(&registry, &bus, &ctx).await;
    let completed = next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    let meta = wf_execution_shared::ContextCompressionCompletedMeta::try_from(&completed).unwrap();
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(
        *runner.workflow_ids.lock().unwrap(),
        vec![
            "stub/llm-summary".to_string(),
            "stub/llm-summary-repress".to_string()
        ]
    );
    assert_eq!(meta.blocks.len(), 2);
    stop_trigger_listener(listener).await;
}
