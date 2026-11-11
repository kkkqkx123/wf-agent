use super::*;

#[tokio::test]
async fn agent_hook_trigger_runs_nested_agent_and_writes_back() {
    // Tests run without a bootstrap, so install the built-in compaction
    // limits first: over-budget paths fail loudly on a missing
    // installation instead of silently falling back to defaults.
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
    use wf_agent::entity::AgentLoopEntity;
    use wf_types::trigger::{TriggerCondition, TriggerTemplate};

    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());

    // Mock LLM drives the nested agent loop.
    let gateway = Arc::new(LlmGateway::new());
    let child_mock = Arc::new(MockLlmClient::new());
    child_mock.default(LlmResponseSpec::text("hook result"));
    gateway.register_mock("mock", child_mock.clone());

    // Register a hook-event trigger template with the nested-agent action
    // (metadata condition: the hook event carries `hook` metadata).
    let ts = wf_common::now();
    let _ = wf_core::registry::MutableRegistry::register(
        &registries.trigger_templates,
        "on_agent_hook".to_string(),
        Arc::new(TriggerTemplate {
            name: "on_agent_hook".to_string(),
            description: Some("run a nested agent on hook".to_string()),
            condition: Some(TriggerCondition {
                event_type: "HOOK_TRIGGERED".to_string(),
                event_name: None,
                condition: None,
                metadata: Some(HashMap::from([(
                    "hook".to_string(),
                    serde_json::json!("pre_tool"),
                )])),
                metadata_exists: None,
                execution_prefix: None,
            }),
            action: Some(TriggerAction::ExecuteTriggeredAgentExecution {
                agent_id: "child-agent".to_string(),
                prompt: Some("summarize the hook".to_string()),
                model: Some("mock".to_string()),
                result_variable: Some("hook_agent_result".to_string()),
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
            ..Default::default()
        },
    );
    wait_for_listener(&bus, 1).await;

    // A live parent agent loop the event points at.
    let parent = Arc::new(AgentLoopEntity::new(Id::from("parent-loop".to_string())));
    let _ = agent_executor.agent_registry().register(parent.clone());

    bus.publish(BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::HookTriggered,
        timestamp: wf_common::now(),
        event_name: None,
        workflow_id: None,
        execution_id: Some(Id::from("parent-loop".to_string())),
        agent_loop_id: Some(Id::from("parent-loop".to_string())),
        metadata: Some(HashMap::from([(
            "hook".to_string(),
            serde_json::json!("pre_tool"),
        )])),
    })
    .unwrap();

    // The child ran against the mock and its result was written back
    // into the parent's variable snapshot.
    wait_until(|| child_mock.recorded_count() >= 1).await;
    {
        let state = parent.state.read().await;
        let snapshots = state.variable_snapshots();
        assert_eq!(
            snapshots.get("hook_agent_result"),
            Some(&Value::from("hook result"))
        );
    }

    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn nested_agent_uses_anchor_snapshot_and_publishes_conversation_writeback() {
    // Tests run without a bootstrap, so install the built-in compaction
    // limits first: over-budget paths fail loudly on a missing
    // installation instead of silently falling back to defaults.
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
    use wf_agent::entity::AgentLoopEntity;
    use wf_types::trigger::{TriggerCondition, TriggerTemplate};

    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());

    let gateway = Arc::new(LlmGateway::new());
    let child_mock = Arc::new(MockLlmClient::new());
    child_mock.default(LlmResponseSpec::text("anchored child result"));
    gateway.register_mock("mock", child_mock.clone());

    let ts = wf_common::now();
    let _ = wf_core::registry::MutableRegistry::register(
        &registries.trigger_templates,
        "on_iteration".to_string(),
        Arc::new(TriggerTemplate {
            name: "on_iteration".to_string(),
            description: Some("run a nested agent on an iteration boundary".to_string()),
            condition: Some(TriggerCondition {
                event_type: "AGENT_ITERATION_COMPLETED".to_string(),
                event_name: None,
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            }),
            action: Some(TriggerAction::ExecuteTriggeredAgentExecution {
                agent_id: "child-agent".to_string(),
                prompt: Some("continue from the parent context".to_string()),
                model: Some("mock".to_string()),
                result_variable: Some("iter_agent_result".to_string()),
                wait_for_completion: Some(true),
                timeout: Some(5000),
                input_mode: Some(wf_types::trigger::TriggerAgentInputMode::PrefixToAnchor),
                writeback: Some(wf_types::trigger::TriggerAgentWriteback::ConversationAppend),
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
            ..Default::default()
        },
    );
    wait_for_listener(&bus, 1).await;
    // The listener's subscription is live only after `wait_for_listener`
    // above (the listener is the first receiver); subscribing now keeps
    // the write-back assertion below deterministic.
    let mut sub = bus.subscribe();

    // A live parent agent loop with a seeded conversation; the trigger
    // event anchors at its current position.
    let parent = Arc::new(AgentLoopEntity::new(Id::from("anchor-loop".to_string())));
    parent
        .conversation()
        .write()
        .await
        .add_message(text_message(MessageRole::User, "parent context message"));
    let (message_count, array_version) = {
        let conv = parent.conversation().read().await;
        (conv.messages().len(), conv.conversation_version())
    };
    let _ = agent_executor.agent_registry().register(parent.clone());

    bus.publish(BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::AgentIterationCompleted,
        timestamp: wf_common::now(),
        event_name: None,
        workflow_id: None,
        execution_id: Some(Id::from("anchor-loop".to_string())),
        agent_loop_id: Some(Id::from("anchor-loop".to_string())),
        metadata: Some(HashMap::from([
            ("iteration".to_string(), serde_json::json!(1)),
            (
                "message_count".to_string(),
                serde_json::json!(message_count),
            ),
            (
                "array_version".to_string(),
                serde_json::json!(array_version),
            ),
        ])),
    })
    .unwrap();

    // The child ran with the parent snapshot: its first request carries
    // the anchored parent message before the child's own prompt.
    wait_until(|| child_mock.recorded_count() >= 1).await;
    let child_request = child_mock.last_request().unwrap();
    assert!(
        child_request.messages.iter().any(|m| matches!(
            &m.content,
            MessageContentValue::Text(t) if t.contains("parent context message")
        )),
        "child must receive the anchored parent conversation snapshot"
    );

    // The variable fall-back write-back happened.
    assert!(
        wf_common::poll_until(
            Duration::from_millis(10),
            Duration::from_secs(2),
            || async {
                parent
                    .state
                    .read()
                    .await
                    .variable_snapshots()
                    .contains_key("iter_agent_result")
            },
        )
        .await,
        "variable fall-back write-back did not land within 2s"
    );
    {
        let state = parent.state.read().await;
        assert_eq!(
            state.variable_snapshots().get("iter_agent_result"),
            Some(&Value::from("anchored child result"))
        );
    }

    // The versioned conversation write-back event was published with the
    // anchor version.
    let writeback_event = loop {
        match sub.recv().await {
            Ok(event) if event.r#type == EventType::ConversationWritebackCompleted => break event,
            Ok(_) => continue,
            Err(_) => panic!("event bus closed"),
        }
    };
    let meta = wf_execution_shared::ConversationWritebackCompletedMeta::try_from(&writeback_event)
        .unwrap();
    assert_eq!(meta.array_version, array_version);
    assert_eq!(
        meta.operation,
        wf_execution_shared::WRITEBACK_OPERATION_APPEND
    );
    assert_eq!(meta.messages.len(), 1);

    stop_trigger_listener(listener).await;
}
