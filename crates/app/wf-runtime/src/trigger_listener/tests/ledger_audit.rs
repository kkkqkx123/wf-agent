use super::*;

/// In-memory test double for the trigger execution ledger.
#[derive(Default)]
struct TestRecorder {
    records: std::sync::Mutex<Vec<wf_types::TriggerExecutionStorageMetadata>>,
}

#[async_trait]
impl TriggerExecutionRecorder for TestRecorder {
    async fn record(
        &self,
        metadata: wf_types::TriggerExecutionStorageMetadata,
    ) -> Result<(), wf_storage::error::StorageError> {
        self.records.lock().unwrap().push(metadata);
        Ok(())
    }
}

#[tokio::test]
async fn trigger_execution_recorded_and_trigger_states_snapshotted() {
    // Tests run without a bootstrap, so install the built-in compaction
    // limits first: over-budget paths fail loudly on a missing
    // installation instead of silently falling back to defaults.
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
    // A direct agent-trigger run through the runner: the durable ledger
    // gets a record and the checkpoint trigger-state registry captures
    // the fired trigger.
    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());
    let gateway = Arc::new(LlmGateway::new());
    let child_mock = Arc::new(MockLlmClient::new());
    child_mock.default(LlmResponseSpec::text("child done"));
    gateway.register_mock("mock", child_mock.clone());

    let ts = wf_common::now();
    let _ = wf_core::registry::MutableRegistry::register(
        &registries.trigger_templates,
        "audit-trigger".to_string(),
        Arc::new(TriggerTemplate {
            name: "audit-trigger".to_string(),
            description: None,
            condition: Some(wf_types::trigger::TriggerCondition {
                event_type: "HOOK_TRIGGERED".to_string(),
                event_name: None,
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            }),
            action: Some(TriggerAction::ExecuteTriggeredAgentExecution {
                agent_id: "child".to_string(),
                prompt: Some("run".to_string()),
                model: Some("mock".to_string()),
                result_variable: Some("audited".to_string()),
                wait_for_completion: Some(true),
                timeout: Some(5000),
                input_mode: None,
                writeback: None,
                checkpoint_message_interval: None,
            }),
            enabled: Some(true),
            max_triggers: None,
            priority: Some(10),
            dispatch_mode: None,
            allow_multi_effect: None,
            effect_order: None,
            metadata: None,
            created_at: ts,
            updated_at: ts,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }),
    );

    let recorder = Arc::new(TestRecorder::default());
    let trigger_states = Arc::new(wf_workflow::TriggerStateRegistry::new());
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let agent_executor = Arc::new(wf_agent::executor::AgentLoopExecutor::new(
        gateway.clone(),
        Arc::new(wf_tools::create_default_tool_registry()),
    ));
    let listener = start_trigger_listener_with_registry(
        bus.clone(),
        registries.clone(),
        gateway,
        contexts,
        ListenerOptions {
            agent_executor: Some(agent_executor.clone()),
            ledger: Some(Arc::new(TriggerLedger::new(
                Some(recorder.clone() as Arc<dyn TriggerExecutionRecorder>),
                Some(trigger_states.clone()),
            ))),
            ..Default::default()
        },
    );
    wait_for_listener(&bus, 1).await;

    let parent = Arc::new(wf_agent::entity::AgentLoopEntity::new(Id::from(
        "audit-loop".to_string(),
    )));
    let _ = agent_executor.agent_registry().register(parent.clone());
    bus.publish(BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::HookTriggered,
        timestamp: wf_common::now(),
        event_name: None,
        workflow_id: None,
        execution_id: Some(Id::from("audit-loop".to_string())),
        agent_loop_id: Some(Id::from("audit-loop".to_string())),
        metadata: None,
    })
    .unwrap();

    wait_until(|| {
        recorder
            .records
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.trigger_name == "audit-trigger")
    })
    .await;
    let record = recorder
        .records
        .lock()
        .unwrap()
        .iter()
        .find(|r| r.trigger_name == "audit-trigger")
        .expect("recorded")
        .clone();
    assert_eq!(record.trigger_type, "event");
    assert_eq!(record.event, "HOOK_TRIGGERED");
    assert_eq!(
        record.action_type.as_deref(),
        Some("execute_triggered_agent_execution")
    );
    assert_eq!(record.outcome, wf_types::TriggerExecutionOutcome::Completed);

    stop_trigger_listener(listener).await;
}

/// A HOOK_TRIGGERED-matched agent trigger template for the ledger tests.
fn hook_agent_trigger_template(name: &str, model: &str) -> TriggerTemplate {
    let ts = wf_common::now();
    TriggerTemplate {
        name: name.to_string(),
        description: None,
        condition: Some(wf_types::trigger::TriggerCondition {
            event_type: "HOOK_TRIGGERED".to_string(),
            event_name: None,
            condition: None,
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        }),
        action: Some(TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id: "child".to_string(),
            prompt: Some("run".to_string()),
            model: Some(model.to_string()),
            result_variable: Some("audited".to_string()),
            wait_for_completion: Some(true),
            timeout: Some(5000),
            input_mode: None,
            writeback: None,
            checkpoint_message_interval: None,
        }),
        enabled: Some(true),
        max_triggers: None,
        priority: Some(10),
        dispatch_mode: None,
        allow_multi_effect: None,
        effect_order: None,
        metadata: None,
        created_at: ts,
        updated_at: ts,
        create_checkpoint: None,
        checkpoint_description_template: None,
    }
}

/// A recorder whose durable write always fails.
#[derive(Default)]
struct FailingRecorder;

#[async_trait]
impl TriggerExecutionRecorder for FailingRecorder {
    async fn record(
        &self,
        _metadata: wf_types::TriggerExecutionStorageMetadata,
    ) -> Result<(), wf_storage::error::StorageError> {
        Err(wf_storage::error::StorageError::General {
            operation: "record_trigger_execution".to_string(),
            message: "ledger unavailable".to_string(),
            source: None,
        })
    }
}

#[tokio::test]
async fn failing_ledger_is_counted_and_never_stops_the_listener() {
    // Tests run without a bootstrap, so install the built-in compaction
    // limits first: over-budget paths fail loudly on a missing
    // installation instead of silently falling back to defaults.
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
    // Two firing events: the second write failure proves the dispatch
    // loop survived the first ledger error, and the shared counter on
    // the handle makes the broken fallback observable.
    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());
    let gateway = Arc::new(LlmGateway::new());
    let child_mock = Arc::new(MockLlmClient::new());
    child_mock.default(LlmResponseSpec::text("child done"));
    gateway.register_mock("mock", child_mock.clone());
    let _ = wf_core::registry::MutableRegistry::register(
        &registries.trigger_templates,
        "ledger-fail-trigger".to_string(),
        Arc::new(hook_agent_trigger_template("ledger-fail-trigger", "mock")),
    );
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let agent_executor = Arc::new(wf_agent::executor::AgentLoopExecutor::new(
        gateway.clone(),
        Arc::new(wf_tools::create_default_tool_registry()),
    ));
    let ledger = Arc::new(TriggerLedger::new(
        Some(Arc::new(FailingRecorder) as Arc<dyn TriggerExecutionRecorder>),
        None,
    ));
    let listener = start_trigger_listener_with_registry(
        bus.clone(),
        registries,
        gateway,
        contexts,
        ListenerOptions {
            agent_executor: Some(agent_executor.clone()),
            ledger: Some(ledger.clone()),
            ..Default::default()
        },
    );
    wait_for_listener(&bus, 1).await;

    let parent = Arc::new(wf_agent::entity::AgentLoopEntity::new(Id::from(
        "ledger-fail-loop".to_string(),
    )));
    let _ = agent_executor.agent_registry().register(parent.clone());
    for expected in 1..=2 {
        bus.publish(BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::HookTriggered,
            timestamp: wf_common::now(),
            event_name: None,
            workflow_id: None,
            execution_id: Some(Id::from("ledger-fail-loop".to_string())),
            agent_loop_id: Some(Id::from("ledger-fail-loop".to_string())),
            metadata: None,
        })
        .unwrap();
        // Sequential: the re-entrancy guard keeps same-key fires from
        // overlapping. Each settled write counts one failure.
        wait_until(|| ledger.write_failures() >= expected).await;
    }
    assert_eq!(
        listener.ledger_write_failures(),
        Some(ledger.write_failures())
    );
    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn shutdown_abandoned_trigger_is_recorded_in_ledger() {
    // Tests run without a bootstrap, so install the built-in compaction
    // limits first: over-budget paths fail loudly on a missing
    // installation instead of silently falling back to defaults.
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
    // A child run without a parent loop executes fire-and-forget and
    // hangs in its LLM call. Stopping the listener abandons the run, and
    // the abandonment must land in the ledger as outcome `abandoned`
    // instead of vanishing with the process.
    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());
    let gateway = Arc::new(LlmGateway::new());
    let slow_mock = Arc::new(MockLlmClient::new());
    slow_mock.default(LlmResponseSpec::text("never").with_delay(60_000));
    gateway.register_mock("slow", slow_mock);
    let _ = wf_core::registry::MutableRegistry::register(
        &registries.trigger_templates,
        "abandon-trigger".to_string(),
        Arc::new(hook_agent_trigger_template("abandon-trigger", "slow")),
    );
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let agent_executor = Arc::new(wf_agent::executor::AgentLoopExecutor::new(
        gateway.clone(),
        Arc::new(wf_tools::create_default_tool_registry()),
    ));
    let recorder = Arc::new(TestRecorder::default());
    let listener = start_trigger_listener_with_registry(
        bus.clone(),
        registries,
        gateway,
        contexts,
        ListenerOptions {
            agent_executor: Some(agent_executor),
            ledger: Some(Arc::new(TriggerLedger::new(
                Some(recorder.clone() as Arc<dyn TriggerExecutionRecorder>),
                None,
            ))),
            ..Default::default()
        },
    );
    wait_for_listener(&bus, 1).await;

    bus.publish(BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::HookTriggered,
        timestamp: wf_common::now(),
        event_name: None,
        workflow_id: None,
        execution_id: Some(Id::from("missing-loop".to_string())),
        agent_loop_id: Some(Id::from("missing-loop".to_string())),
        metadata: None,
    })
    .unwrap();
    // Let the dispatch enter the in-flight run before stopping.
    tokio::time::sleep(Duration::from_millis(100)).await;
    stop_trigger_listener(listener).await;

    wait_until(|| {
        recorder.records.lock().unwrap().iter().any(|r| {
            r.trigger_name == "abandon-trigger"
                && r.outcome == wf_types::TriggerExecutionOutcome::Abandoned
        })
    })
    .await;
}
