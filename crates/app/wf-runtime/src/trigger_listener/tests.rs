use super::*;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::hooks::HookContext;
use wf_llm::mock::{LlmResponseSpec, MockLlmClient};
use wf_resource::registry::RegisterOptions;
use wf_types::events::BaseEvent;
use wf_types::events::EventType;
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::node::StaticNodeType;
use wf_types::trigger::{TriggerAction, TriggerTemplate};
use wf_types::workflow::EdgeType;
use wf_types::workflow::WorkflowTemplate;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_types::Id;
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::{WorkflowCoordinator, WorkflowExecutionEntity};

fn text_message(role: MessageRole, text: &str) -> Message {
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

/// Wait until the bus sees the expected number of receivers (the
/// listener subscribes on its first poll). Bounded: a wrong expectation
/// must fail loudly instead of spinning forever.
async fn wait_for_listener(bus: &EventBus, expected_receivers: usize) {
    assert!(
        wf_common::poll_until(Duration::from_millis(10), Duration::from_secs(2), || {
            let matched = bus.total_receiver_count() >= expected_receivers;
            async move { matched }
        },)
        .await,
        "expected {expected_receivers} receivers within 2s, got {}",
        bus.total_receiver_count()
    );
}

/// Poll a condition until it holds (2s budget).
async fn wait_until(cond: impl Fn() -> bool) {
    assert!(
        wf_common::poll_until(Duration::from_millis(10), Duration::from_secs(2), || {
            let matched = cond();
            async move { matched }
        },)
        .await,
        "condition not reached within budget"
    );
}

fn node(id: &str, node_type: &str, inner: Value) -> WorkflowNode {
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: node_type.to_string(),
        inner,
    }
}

fn edge(source: &str, target: &str) -> WorkflowEdge {
    WorkflowEdge {
        id: format!("{}-{}", source, target),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        r#type: EdgeType::Default,
        condition: None,
        label: None,
        description: None,
        error_route: None,
    }
}

fn workflow_options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(false),
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

#[tokio::test]
async fn agent_hook_trigger_runs_nested_agent_and_writes_back() {
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
            Ok(event) if event.r#type == EventType::ConversationWritebackCompleted => {
                break event
            }
            Ok(_) => continue,
            Err(_) => panic!("event bus closed"),
        }
    };
    let meta =
        wf_execution_shared::ConversationWritebackCompletedMeta::try_from(&writeback_event)
            .unwrap();
    assert_eq!(meta.array_version, array_version);
    assert_eq!(
        meta.operation,
        wf_execution_shared::WRITEBACK_OPERATION_APPEND
    );
    assert_eq!(meta.messages.len(), 1);

    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn context_compression_chain_end_to_end() {
    // 1. Components: bus + predefined resources. Only the summary
    // workflow template is registered here: the compression chain is
    // served by the hook-handler fire (the preset trigger template
    // was removed from the trigger system).
    let bus = Arc::new(EventBus::new(256));
    let mut sub = bus.subscribe();

    let registries = Arc::new(ResourceRegistries::new());
    let opts = RegisterOptions::default();
    wf_resource::predefined::workflow::register(&registries, &opts);

    // 2. Mock LLM: "main" for the emitting node, "DEFAULT" for the
    // @standard/fold-summary node.
    let gateway = Arc::new(LlmGateway::new());
    let main_mock = Arc::new(MockLlmClient::new());
    main_mock.default(LlmResponseSpec::text("main answer").with_usage(100, 20));
    gateway.register_mock("main", main_mock);
    let summary_mock = Arc::new(MockLlmClient::new());
    summary_mock.default(LlmResponseSpec::text("compressed summary").with_usage(50, 30));
    gateway.register_mock("DEFAULT", summary_mock.clone());
    // Context budgets derive from the model window only: the emitting
    // node resolves its budget from the "main" profile window.
    for (id, window) in [("main", 2000u32), ("DEFAULT", 200_000u32)] {
        gateway
            .profile_registry()
            .register(wf_types::llm::LlmProfile {
                id: id.to_string(),
                name: id.to_string(),
                format: wf_types::llm::LlmFormat::OpenaiChat,
                provider_id: None,
                model: "mock-model".to_string(),
                api_key: None,
                base_url: None,
                parameters: None,
                generation: None,
                timeout: None,
                max_retries: None,
                retry_delay: None,
                headers: None,
                metadata: None,
                tool_call_protocol: None,
                auth_type: None,
                custom_headers: None,
                custom_body: None,
                custom_body_enabled: None,
                query_params: None,
                stream_options: None,
                context_window_size: Some(window),
            })
            .expect("test profile registers");
    }

    // 3. Wire the hook registry with the builtin compression handler:
    // the LLM handler fires the compression signal synchronously
    // and the service spawns the summary sub-workflow immediately.
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let hook_handler_registry = Arc::new(HookHandlerRegistry::new());
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_tool_registry(
        registries.clone(),
        bus.clone(),
        gateway.clone(),
        contexts.clone(),
        None,
        None,
    ));
    register_compression_handler(
        &hook_handler_registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            runner,
            contexts: contexts.clone(),
            summary_workflow_id: wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID
                .to_string(),
            shutdown: CancellationToken::new(),
            ledger: None,
            policy: CompressionPolicy::default(),
        },
    );

    // 4. Main workflow: an LLM node reading the "chat" named context
    // whose estimated token count exceeds the node-level limit.
    let execution_id = wf_common::generate_id();
    let variables = Arc::new(DashMap::new());
    let mut chat_messages = Vec::new();
    for i in 0..40 {
        chat_messages.push(text_message(
            MessageRole::User,
            &format!("long message {} {}", i, "x".repeat(200)),
        ));
    }
    wf_workflow::append_context(&variables, "chat", chat_messages);
    contexts.register_workflow(execution_id.clone(), variables.clone());

    let graph = WorkflowGraphStructure {
        nodes: vec![
            node("start", "START", Value::Null),
            node(
                "llm",
                "LLM",
                serde_json::json!({
                    "profile_id": "main",
                    "context_id": "chat",
                    "token_limit": 1000,
                    "output_context": "chat_output",
                }),
            ),
            node("end", "END", Value::Null),
        ],
        edges: vec![edge("start", "llm"), edge("llm", "end")],
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    };
    let handlers = wf_workflow::create_default_handlers(gateway.clone(), None);
    let exec_ctx = ExecutorContext::new(
        execution_id.clone(),
        wf_common::generate_id(),
        Some(bus.clone()),
        Arc::new(wf_tools::create_default_tool_registry()),
        workflow_options(),
    );
    let mut exec_ctx = exec_ctx;
    exec_ctx.variables = variables.clone();
    exec_ctx = exec_ctx.with_hook_handler_registry(hook_handler_registry.clone());
    let entity = WorkflowExecutionEntity::new(
        exec_ctx.execution_id.clone(),
        exec_ctx.workflow_id.clone(),
    );
    let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
        .unwrap()
        .with_entity(entity);
    assert!(coordinator.execute().await.is_ok());

    // 5a. CONTEXT_COMPRESSION_REQUESTED names the "chat" array and
    // reports its accounting; the audit copy carries no snapshot.
    let requested = loop {
        match sub.recv().await {
            Ok(event) if event.r#type == EventType::ContextCompressionRequested => break event,
            Ok(_) => continue,
            Err(_) => panic!("event bus closed"),
        }
    };
    assert_eq!(
        requested.execution_id.as_deref(),
        Some(execution_id.as_str())
    );
    let requested_meta =
        wf_execution_shared::ContextCompressionRequestedMeta::try_from(&requested).unwrap();
    assert_eq!(requested_meta.target_context_id, "chat");
    assert_eq!(
        requested_meta.message_count, 40,
        "event must report the array size"
    );
    assert!(
        !requested
            .metadata
            .as_ref()
            .is_some_and(|m| m.contains_key(wf_execution_shared::KEY_MESSAGES)),
        "the audit copy must not carry the snapshot"
    );

    // 5b. The summary workflow ran over a head-trimmed window of the
    // conversation: the service trims the snapshot below the token
    // limit before summarizing, and the trim keeps the tail.
    wait_until(|| summary_mock.recorded_count() >= 1).await;
    let summary_request = summary_mock.last_request().unwrap();
    assert!(
        !summary_request.messages.is_empty() && summary_request.messages.len() < 40,
        "summary input must be a trimmed window of the conversation"
    );
    let expected_tail_content =
        MessageContentValue::Text(format!("long message 39 {}", "x".repeat(200)));
    assert_eq!(
        summary_request.messages.last().map(|m| &m.content),
        Some(&expected_tail_content),
        "summary input must retain the conversation tail"
    );

    // 5c. The compressed array was written back: summary first, then
    // the retained tail (default tail_keep keeps recent messages
    // visible alongside the summary).
    let expected_len = 1 + wf_execution_shared::DEFAULT_COMPRESSION_TAIL_KEEP;
    wait_until(|| {
        let written = wf_workflow::get_context(&variables, "chat");
        written.len() == expected_len && written[0].role == MessageRole::Assistant
    })
    .await;
    let written = wf_workflow::get_context(&variables, "chat");
    assert_eq!(written.len(), expected_len);
    assert_eq!(
        written[0].content,
        MessageContentValue::Text("compressed summary".to_string())
    );
    for tail in &written[1..] {
        assert_eq!(tail.role, MessageRole::User);
    }

    // 5d. CONTEXT_COMPRESSION_COMPLETED carries the compressed array
    // (summary only; tail retention is a write-back concern).
    let completed = loop {
        match sub.recv().await {
            Ok(event) if event.r#type == EventType::ContextCompressionCompleted => break event,
            Ok(_) => continue,
            Err(_) => panic!("event bus closed"),
        }
    };
    let completed_meta =
        wf_execution_shared::ContextCompressionCompletedMeta::try_from(&completed).unwrap();
    assert_eq!(completed_meta.target_context_id, "chat");
    assert_eq!(completed_meta.messages.len(), 1);
    assert_eq!(
        completed_meta.messages[0].content,
        MessageContentValue::Text("compressed summary".to_string())
    );
    assert_eq!(
        completed_meta.tail_keep,
        wf_execution_shared::DEFAULT_COMPRESSION_TAIL_KEEP
    );
    assert!(
        completed_meta.tokens_after < 1000,
        "compressed array must be far below the limit"
    );

    contexts.unregister(&execution_id);
}

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

#[tokio::test]
async fn no_compression_event_when_named_array_within_limit() {
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();

    let registries = Arc::new(ResourceRegistries::new());
    let opts = RegisterOptions::default();
    wf_resource::predefined::workflow::register(&registries, &opts);

    let gateway = Arc::new(LlmGateway::new());
    let main_mock = Arc::new(MockLlmClient::new());
    main_mock.default(LlmResponseSpec::text("main answer").with_usage(100, 20));
    gateway.register_mock("main", main_mock);
    let summary_mock = Arc::new(MockLlmClient::new());
    summary_mock.default(LlmResponseSpec::text("compressed summary"));
    gateway.register_mock("DEFAULT", summary_mock.clone());
    for (id, window) in [("main", 2000u32), ("DEFAULT", 200_000u32)] {
        gateway
            .profile_registry()
            .register(wf_types::llm::LlmProfile {
                id: id.to_string(),
                name: id.to_string(),
                format: wf_types::llm::LlmFormat::OpenaiChat,
                provider_id: None,
                model: "mock-model".to_string(),
                api_key: None,
                base_url: None,
                parameters: None,
                generation: None,
                timeout: None,
                max_retries: None,
                retry_delay: None,
                headers: None,
                metadata: None,
                tool_call_protocol: None,
                auth_type: None,
                custom_headers: None,
                custom_body: None,
                custom_body_enabled: None,
                query_params: None,
                stream_options: None,
                context_window_size: Some(window),
            })
            .expect("test profile registers");
    }

    let contexts = Arc::new(ExecutionContextRegistry::new());
    let hook_handler_registry = Arc::new(HookHandlerRegistry::new());
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_tool_registry(
        registries.clone(),
        bus.clone(),
        gateway.clone(),
        contexts.clone(),
        None,
        None,
    ));
    register_compression_handler(
        &hook_handler_registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            runner,
            contexts: contexts.clone(),
            summary_workflow_id: wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID
                .to_string(),
            shutdown: CancellationToken::new(),
            ledger: None,
            policy: CompressionPolicy::default(),
        },
    );

    // A short array stays within the limit: no compression requested.
    let execution_id = wf_common::generate_id();
    let variables = Arc::new(DashMap::new());
    wf_workflow::append_context(
        &variables,
        "chat",
        vec![text_message(MessageRole::User, "short")],
    );
    contexts.register_workflow(execution_id.clone(), variables.clone());

    let graph = WorkflowGraphStructure {
        nodes: vec![
            node("start", "START", Value::Null),
            node(
                "llm",
                "LLM",
                serde_json::json!({
                    "profile_id": "main",
                    "context_id": "chat",
                    "token_limit": 1000,
                    "output_context": "chat_output",
                }),
            ),
            node("end", "END", Value::Null),
        ],
        edges: vec![edge("start", "llm"), edge("llm", "end")],
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    };
    let handlers = wf_workflow::create_default_handlers(gateway.clone(), None);
    let exec_ctx = ExecutorContext::new(
        execution_id.clone(),
        wf_common::generate_id(),
        Some(bus.clone()),
        Arc::new(wf_tools::create_default_tool_registry()),
        workflow_options(),
    );
    let mut exec_ctx = exec_ctx;
    exec_ctx.variables = variables.clone();
    exec_ctx = exec_ctx.with_hook_handler_registry(hook_handler_registry.clone());
    let entity = WorkflowExecutionEntity::new(
        exec_ctx.execution_id.clone(),
        exec_ctx.workflow_id.clone(),
    );
    let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
        .unwrap()
        .with_entity(entity);
    assert!(coordinator.execute().await.is_ok());

    // No compression request within the observation window.
    let compression_observed = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            match sub.recv().await {
                Ok(event) if event.r#type == EventType::ContextCompressionRequested => {
                    return true
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(
        !compression_observed,
        "in-limit array must not trigger compression"
    );
    assert_eq!(summary_mock.recorded_count(), 0);

    contexts.unregister(&execution_id);
}

#[tokio::test]
async fn compression_fire_takes_over_immediately() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use wf_execution_shared::hooks::fire;
    use wf_execution_shared::token_events::{
        KEY_ARRAY_VERSION, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_TARGET_CONTEXT_ID,
        KEY_TOKENS_USED, KEY_TOKEN_LIMIT,
    };

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
    register_compression_handler(
        &hook_handler_registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            runner: Arc::new(StuckRunner(started.clone())),
            contexts: contexts.clone(),
            summary_workflow_id: wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID
                .to_string(),
            shutdown: CancellationToken::new(),
            ledger: None,
            policy: CompressionPolicy::default(),
        },
    );

    // A valid compression payload (message snapshot present).
    let messages: Vec<Message> = vec![text_message(MessageRole::User, "long message")];
    let mut data = HashMap::new();
    data.insert(KEY_TARGET_CONTEXT_ID.to_string(), Value::from("chat"));
    data.insert(KEY_TOKENS_USED.to_string(), Value::from(900u64));
    data.insert(KEY_TOKEN_LIMIT.to_string(), Value::from(1000u64));
    data.insert(KEY_MESSAGE_COUNT.to_string(), Value::from(messages.len()));
    data.insert(KEY_ARRAY_VERSION.to_string(), Value::from(1u64));
    data.insert(
        KEY_MESSAGES.to_string(),
        serde_json::to_value(messages).unwrap(),
    );
    let ctx = HookContext {
        execution_id: Id::from("wf-run".to_string()),
        hook_type: wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
        data,
        cancellation: tokio_util::sync::CancellationToken::new(),
    };

    // Fire returns as soon as the summary sub-workflow is spawned,
    // never after it completes (the stub blocks 10s).
    let elapsed = std::time::Instant::now();
    fire(
        &hook_handler_registry,
        &[],
        wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        &ctx,
        Some(&bus),
    )
    .await;
    let fire_ms = elapsed.elapsed().as_millis();
    assert!(
        fire_ms < 2000,
        "fire must return at takeover, not after compression (took {fire_ms}ms)"
    );
    // The spawned summary task is observable as soon as the runtime
    // schedules it: fire did not await it (it is still blocked in
    // the stub for the rest of the test).
    wait_until(|| started.load(Ordering::SeqCst)).await;
}

/// Stub summary runner: fails the first `fail_first` attempts, then
/// returns a compressed message array. With an agent-target signal
/// (`agent_loop_id` present) the write-back skips the variable map, so
/// the chain policy is exercised without a live execution context.
struct FlakySummaryRunner {
    fail_first: u32,
    calls: std::sync::atomic::AtomicU32,
}

#[async_trait]
impl SubworkflowRunner for FlakySummaryRunner {
    async fn run(&self, _workflow_id: &str, _input: Value) -> WorkflowResult<Value> {
        let attempt = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if attempt <= self.fail_first {
            return Err(WorkflowError::TriggerError(format!(
                "stub summary failure {attempt}"
            )));
        }
        Ok(serde_json::to_value(vec![text_message(
            MessageRole::Assistant,
            "compressed summary",
        )])
        .expect("message array serializes"))
    }
}

/// Stub summary runner whose first attempt hangs: the attempt-timeout
/// containment must interrupt it and the chain must still retry.
struct HangingFirstRunner {
    calls: std::sync::atomic::AtomicU32,
}

#[async_trait]
impl SubworkflowRunner for HangingFirstRunner {
    async fn run(&self, _workflow_id: &str, _input: Value) -> WorkflowResult<Value> {
        let attempt = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if attempt == 1 {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
        Ok(serde_json::to_value(vec![text_message(
            MessageRole::Assistant,
            "compressed summary",
        )])
        .expect("message array serializes"))
    }
}

/// One agent-target compression signal (version 7, non-empty snapshot).
fn agent_compression_signal(messages: &[Message]) -> HookContext {
    use wf_execution_shared::token_events::{
        KEY_ARRAY_VERSION, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_TARGET_CONTEXT_ID,
        KEY_TOKENS_USED, KEY_TOKEN_LIMIT,
    };
    let mut data = HashMap::new();
    data.insert(KEY_TARGET_CONTEXT_ID.to_string(), Value::from("chat"));
    data.insert(KEY_TOKENS_USED.to_string(), Value::from(900u64));
    data.insert(KEY_TOKEN_LIMIT.to_string(), Value::from(1000u64));
    data.insert(KEY_MESSAGE_COUNT.to_string(), Value::from(messages.len()));
    data.insert(KEY_ARRAY_VERSION.to_string(), Value::from(7u64));
    data.insert(
        KEY_MESSAGES.to_string(),
        serde_json::to_value(messages).expect("snapshot serializes"),
    );
    data.insert("agent_loop_id".to_string(), Value::from("loop-1"));
    HookContext {
        execution_id: Id::from("retry-run".to_string()),
        hook_type: wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
        data,
        cancellation: tokio_util::sync::CancellationToken::new(),
    }
}

fn stub_compression_policy(
    max_retries: u32,
    run_timeout_ms: u64,
    fallback: wf_types::workflow::CompressionFallbackMode,
) -> CompressionPolicy {
    CompressionPolicy {
        tail_keep: 2,
        max_retries,
        run_timeout_ms,
        fallback,
    }
}

async fn fire_compression_signal(
    registry: &Arc<HookHandlerRegistry>,
    bus: &Arc<EventBus>,
    ctx: &HookContext,
) {
    wf_execution_shared::hooks::fire(
        registry,
        &[],
        wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        ctx,
        Some(bus),
    )
    .await;
}

/// Drain events until one of the given types arrives; a FAILED event
/// arriving where success is expected (or vice versa) fails the test.
async fn next_compression_event(
    sub: &mut wf_core::Subscription,
    wanted: EventType,
) -> BaseEvent {
    loop {
        match sub.recv().await {
            Ok(event) if event.r#type == wanted => return event,
            Ok(event)
                if event.r#type == EventType::ContextCompressionCompleted
                    || event.r#type == EventType::ContextCompressionFailed =>
            {
                panic!(
                    "expected {wanted:?} but got {:?}: {:?}",
                    event.r#type, event.metadata
                );
            }
            Ok(_) => continue,
            Err(_) => panic!("event bus closed"),
        }
    }
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
    let pipeline = super::compression::CompressionPipeline::with_ledger(
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
        },
    );
    let template = super::compression::builtin_compression_template();
    assert_eq!(
        template.name,
        super::compression::BUILTIN_COMPRESSION_TEMPLATE_NAME
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
        ledger: None,
        hook_handler_registry: None,
        signal_bus: None,
        timer_bindings: None,
        schedule_state_store: None,
        shutdown: CancellationToken::new(),
        compression_route: Some(CompressionRouteConfig {
            summary_workflow_id: "stub/llm-summary".to_string(),
            policy: policy.clone(),
        }),
    });
    register_routed_compression_handler(
        &hook_handler_registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            runner: runner.clone(),
            contexts,
            summary_workflow_id: "stub/llm-summary".to_string(),
            shutdown: CancellationToken::new(),
            ledger: None,
            policy,
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
    register_compression_handler(
        &registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            runner: runner.clone(),
            contexts,
            summary_workflow_id: "stub/llm-summary".to_string(),
            shutdown: CancellationToken::new(),
            ledger: None,
            policy: stub_compression_policy(
                1,
                5_000,
                wf_types::workflow::CompressionFallbackMode::Fail,
            ),
        },
    );

    let messages = vec![text_message(MessageRole::User, "long message")];
    fire_compression_signal(&registry, &bus, &agent_compression_signal(&messages)).await;

    next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    // First attempt failed, second succeeded: no FAILED event landed.
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
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
    register_compression_handler(
        &registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            runner: runner.clone(),
            contexts,
            summary_workflow_id: "stub/llm-summary".to_string(),
            shutdown: CancellationToken::new(),
            ledger: None,
            // A hung attempt must be cut at 300ms, not stall the chain.
            policy: stub_compression_policy(
                1,
                300,
                wf_types::workflow::CompressionFallbackMode::Fail,
            ),
        },
    );

    let messages = vec![text_message(MessageRole::User, "long message")];
    fire_compression_signal(&registry, &bus, &agent_compression_signal(&messages)).await;

    next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn compression_duplicate_signal_same_version_runs_once() {
    // Claim mechanics live on the synchronous takeover path: a second
    // identical fire while the first chain is still in flight is skipped,
    // so the stub runner observes exactly one chain (hung attempt cut at
    // the attempt timeout, then the retry success).
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let registry = Arc::new(HookHandlerRegistry::new());
    let runner = Arc::new(HangingFirstRunner {
        calls: std::sync::atomic::AtomicU32::new(0),
    });
    register_compression_handler(
        &registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            runner: runner.clone(),
            contexts,
            summary_workflow_id: "stub/llm-summary".to_string(),
            shutdown: CancellationToken::new(),
            ledger: None,
            policy: stub_compression_policy(
                1,
                300,
                wf_types::workflow::CompressionFallbackMode::Fail,
            ),
        },
    );

    let messages = vec![text_message(MessageRole::User, "long message")];
    let ctx = agent_compression_signal(&messages);
    fire_compression_signal(&registry, &bus, &ctx).await;
    fire_compression_signal(&registry, &bus, &ctx).await;

    next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
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
    register_compression_handler(
        &registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            runner: runner.clone(),
            contexts,
            summary_workflow_id: "stub/llm-summary".to_string(),
            shutdown: CancellationToken::new(),
            ledger: None,
            // `fail` (the default mode): a terminal failure publishes
            // FAILED for external handling instead of landing a
            // degraded window.
            policy: stub_compression_policy(
                1,
                5_000,
                wf_types::workflow::CompressionFallbackMode::Fail,
            ),
        },
    );

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
    register_compression_handler(
        &registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            runner: runner.clone(),
            contexts,
            summary_workflow_id: "stub/llm-summary".to_string(),
            shutdown: CancellationToken::new(),
            ledger: None,
            policy: stub_compression_policy(
                0,
                5_000,
                wf_types::workflow::CompressionFallbackMode::PartialSummary,
            ),
        },
    );

    let messages = vec![
        text_message(MessageRole::User, "oldest long message"),
        text_message(MessageRole::User, "newest long message"),
    ];
    fire_compression_signal(&registry, &bus, &agent_compression_signal(&messages)).await;

    let completed =
        next_compression_event(&mut sub, EventType::ContextCompressionCompleted).await;
    let meta =
        wf_execution_shared::ContextCompressionCompletedMeta::try_from(&completed).unwrap();
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
}

fn template_with_nodes() -> WorkflowTemplate {
    use wf_types::node::BaseStaticNode;
    use wf_types::workflow::{
        Edge, TriggeredSubworkflowConfig, WorkflowDefinition, WorkflowMetadata,
    };
    WorkflowTemplate {
        id: "t_flow".to_string(),
        name: "T Flow".to_string(),
        description: "test".to_string(),
        definition: WorkflowDefinition {
            id: "t_flow".to_string(),
            name: "T Flow".to_string(),
            description: Some("test".to_string()),
            r#type: None,
            version: None,
            nodes: vec![
                BaseStaticNode {
                    id: "start".into(),
                    node_type: StaticNodeType::StartFromMessage,
                    name: Some("Start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "llm".into(),
                    node_type: StaticNodeType::Llm,
                    name: Some("LLM".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "end".into(),
                    node_type: StaticNodeType::ContinueFromMessage,
                    name: Some("End".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![
                Edge {
                    id: "e1".into(),
                    source_node_id: "start".into(),
                    target_node_id: "llm".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                    error_route: None,
                },
                Edge {
                    id: "e2".into(),
                    source_node_id: "llm".into(),
                    target_node_id: "end".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                    error_route: None,
                },
            ],
            config: None,
            variables: None,
            triggered_subworkflow_config: Some(TriggeredSubworkflowConfig {
                enable_checkpoints: Some(false),
                timeout: Some(5000),
                compression_fallback: None,
            }),
            metadata: Some(WorkflowMetadata {
                author: None,
                tags: None,
                category: None,
            }),
            available_tools: None,
            hooks: None,
            created_at: 0,
            updated_at: 0,
        },
        template_category: None,
        template_tags: None,
        is_public: None,
        enabled: None,
    }
}

#[test]
fn template_to_graph_maps_nodes_edges_and_endpoints() {
    let template = template_with_nodes();
    let graph = template_to_graph(&template);

    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.nodes[0].node_type, "START_FROM_MESSAGE");
    assert_eq!(graph.nodes[1].node_type, "LLM");
    assert_eq!(graph.nodes[2].node_type, "CONTINUE_FROM_MESSAGE");
    assert_eq!(graph.start_node_id.as_deref(), Some("start"));
    assert_eq!(graph.end_node_ids, vec!["end".to_string()]);
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(graph.edges[0].source_node_id, "start");
    assert_eq!(graph.edges[1].target_node_id, "end");
}

#[tokio::test]
async fn execution_context_registry_writes_back_to_registered_execution() {
    use wf_workflow::execution_context::WriteBackError;

    let registry = ExecutionContextRegistry::new();
    assert!(!registry.registered("exec-1"));
    assert!(matches!(
        registry.write_context("exec-1", "chat", vec![], 0).await,
        Err(WriteBackError::NotRegistered)
    ));

    let variables = Arc::new(DashMap::new());
    registry.register_workflow("exec-1", variables.clone());
    assert!(registry.registered("exec-1"));

    let msg = Message {
        id: wf_common::generate_id(),
        role: wf_types::message::MessageRole::Assistant,
        content: wf_types::message::MessageContentValue::Text("summary".to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    };
    // An array the execution never created is not writable.
    assert!(matches!(
        registry
            .write_context("exec-1", "chat", vec![msg.clone()], 0)
            .await,
        Err(WriteBackError::ContextNotFound)
    ));
    // Versioned write-back of a tracked array succeeds.
    wf_workflow::append_context(&variables, "chat", vec![msg.clone()]);
    let version = wf_workflow::message_context::array_version(&variables, "chat");
    assert!(registry
        .write_context("exec-1", "chat", vec![msg.clone()], version)
        .await
        .is_ok());
    let written = wf_workflow::get_context(&variables, "chat");
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].content, msg.content);

    registry.unregister("exec-1");
    assert!(!registry.registered("exec-1"));
    assert!(matches!(
        registry.write_context("exec-1", "chat", vec![], 0).await,
        Err(WriteBackError::NotRegistered)
    ));
}

#[tokio::test]
async fn versioned_write_back_discards_stale_compression() {
    use wf_workflow::execution_context::WriteBackError;

    let registry = ExecutionContextRegistry::new();
    let variables = Arc::new(DashMap::new());
    wf_workflow::append_context(
        &variables,
        "chat",
        vec![Message {
            id: wf_common::generate_id(),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text("old".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }],
    );
    registry.register_workflow("exec-2", variables.clone());
    let emitted_version = wf_workflow::message_context::array_version(&variables, "chat");
    assert!(emitted_version > 0);

    // New messages appended after the event was emitted: the array moved
    // past the event version, the compressed result must be discarded.
    wf_workflow::append_context(
        &variables,
        "chat",
        vec![Message {
            id: wf_common::generate_id(),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text("newer".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }],
    );
    assert!(matches!(
        registry
            .write_context(
                "exec-2",
                "chat",
                vec![Message {
                    id: wf_common::generate_id(),
                    role: wf_types::message::MessageRole::Assistant,
                    content: wf_types::message::MessageContentValue::Text(
                        "summary".to_string()
                    ),
                    timestamp: wf_common::now(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: None,
                    thinking: None,
                    metadata: None,
                }],
                emitted_version,
            )
            .await,
        Err(WriteBackError::VersionMismatch { .. })
    ));
    assert_eq!(wf_workflow::get_context(&variables, "chat").len(), 2);

    // At the current version the write-back succeeds.
    let current = wf_workflow::message_context::array_version(&variables, "chat");
    assert!(registry
        .write_context(
            "exec-2",
            "chat",
            vec![Message {
                id: wf_common::generate_id(),
                role: wf_types::message::MessageRole::Assistant,
                content: wf_types::message::MessageContentValue::Text("summary".to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
            current,
        )
        .await
        .is_ok());
    assert_eq!(wf_workflow::get_context(&variables, "chat").len(), 1);
}

#[tokio::test]
async fn context_trigger_sets_variable_on_live_execution() {
    use wf_types::trigger::{TriggerCondition, TriggerTemplate};

    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());
    let gateway = Arc::new(LlmGateway::new());
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let listener =
        start_trigger_listener(bus.clone(), registries.clone(), gateway, contexts.clone());
    wait_for_listener(&bus, 1).await;

    // A SetVariable trigger template matching custom events.
    let ts = wf_common::now();
    let _ = wf_core::registry::MutableRegistry::register(
        &registries.trigger_templates,
        "on_flag".to_string(),
        Arc::new(TriggerTemplate {
            name: "on_flag".to_string(),
            description: Some("set a variable from an event".to_string()),
            condition: Some(TriggerCondition {
                event_type: "NODE_CUSTOM_EVENT".to_string(),
                event_name: Some("flag_raised".to_string()),
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            }),
            action: Some(TriggerAction::SetVariable {
                variable_name: "event_flag".to_string(),
                value: Value::Bool(true),
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

    // A live workflow execution registered in the write-back registry.
    let execution_id = "exec-live-1".to_string();
    let variables = Arc::new(DashMap::new());
    contexts.register_workflow(execution_id.clone(), variables.clone());

    bus.publish(BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::NodeCustomEvent,
        timestamp: wf_common::now(),
        event_name: Some("flag_raised".to_string()),
        workflow_id: Some(Id::from("wf-live-1".to_string())),
        execution_id: Some(execution_id.clone()),
        agent_loop_id: None,
        metadata: None,
    })
    .unwrap();

    // The action ran against the live execution's variable map.
    wait_until(|| variables.contains_key("event_flag")).await;
    assert_eq!(
        variables.get("event_flag").map(|v| v.value().clone()),
        Some(Value::Bool(true))
    );

    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn context_trigger_skips_events_without_live_context() {
    use wf_types::trigger::{TriggerCondition, TriggerTemplate};

    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());
    let gateway = Arc::new(LlmGateway::new());
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let listener =
        start_trigger_listener(bus.clone(), registries.clone(), gateway, contexts.clone());
    wait_for_listener(&bus, 1).await;

    let ts = wf_common::now();
    let _ = wf_core::registry::MutableRegistry::register(
        &registries.trigger_templates,
        "on_flag_2".to_string(),
        Arc::new(TriggerTemplate {
            name: "on_flag_2".to_string(),
            description: None,
            condition: Some(TriggerCondition {
                event_type: "NODE_CUSTOM_EVENT".to_string(),
                event_name: Some("flag_raised".to_string()),
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            }),
            action: Some(TriggerAction::SetVariable {
                variable_name: "event_flag".to_string(),
                value: Value::Bool(true),
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

    // No execution registered: the runner must skip (agent sessions are
    // not registered), never fail the listener loop.
    bus.publish(BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::NodeCustomEvent,
        timestamp: wf_common::now(),
        event_name: Some("flag_raised".to_string()),
        workflow_id: None,
        execution_id: Some("no-such-execution".to_string()),
        agent_loop_id: None,
        metadata: None,
    })
    .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn listener_subscription_exists_synchronously_after_start() {
    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());
    let gateway = Arc::new(LlmGateway::new());
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let listener =
        start_trigger_listener(bus.clone(), registries.clone(), gateway, contexts.clone());
    assert!(
        bus.total_receiver_count() >= 1,
        "subscription must exist synchronously after start (got {})",
        bus.total_receiver_count()
    );

    stop_trigger_listener(listener).await;
}
