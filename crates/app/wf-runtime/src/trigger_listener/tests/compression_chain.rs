use super::*;

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
        file_scope: None,
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
async fn context_compression_chain_end_to_end() {
    // Tests run without a bootstrap, so install the built-in compaction
    // limits first: over-budget paths fail loudly on a missing
    // installation instead of silently falling back to defaults.
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
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
                max_output_tokens: None,
                proxy: None,
                no_proxy: None,
                circuit_breaker: None,
                prompt_caching: None,
                auth: None,
                compat: None,
                identity: None,
                transport: None,
                env: None,
            })
            .expect("test profile registers");
    }

    // 3. Wire the hook registry with the builtin compression handler:
    // the LLM handler fires the compression signal synchronously
    // and the adapter publishes the routed handoff for the listener route.
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let hook_handler_registry = Arc::new(HookHandlerRegistry::new());
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_tool_registry(
        registries.clone(),
        bus.clone(),
        gateway.clone(),
        contexts.clone(),
        None,
        None,
        None,
    ));
    let compression_policy = CompressionPolicy::default();
    register_compression_handler(
        &hook_handler_registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            pipeline: None,
        },
    );
    let listener = start_trigger_listener_with_parts(ListenerDeps {
        event_bus: bus.clone(),
        registries: registries.clone(),
        contexts: contexts.clone(),
        runner: runner.clone(),
        gateway: gateway.clone(),
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
            summary_workflow_id: wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID
                .to_string(),
            repress_workflow_id: None,
            policy: compression_policy,
        }),
    });
    wait_for_listener(&bus, 1).await;

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
    let entity =
        WorkflowExecutionEntity::new(exec_ctx.execution_id.clone(), exec_ctx.workflow_id.clone());
    let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
        .unwrap()
        .with_entity(entity);
    assert!(coordinator.execute().await.is_ok());

    // 5a. CONTEXT_COMPRESSION_REQUESTED names the "chat" array and
    // reports its accounting; the audit copy carries no snapshot. The
    // adapter-published routed handoff shares the event type and arrives
    // first, so the loop skips routed copies and asserts on the audit one.
    let requested = loop {
        match sub.recv().await {
            Ok(event)
                if event.r#type == EventType::ContextCompressionRequested
                    && !wf_execution_shared::is_compression_routed_event(&event) =>
            {
                break event
            }
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

    // 5d. CONTEXT_COMPRESSION_COMPLETED carries the version anchor and
    // accounting; the message array travels through the registry
    // write-back for workflow targets, not the event.
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
    assert!(
        completed_meta.messages.is_empty(),
        "workflow-target completions must not repeat the array on the event"
    );
    assert_eq!(
        completed_meta.tail_keep,
        wf_execution_shared::DEFAULT_COMPRESSION_TAIL_KEEP
    );
    assert!(
        completed_meta.tokens_after < 1000,
        "compressed array must be far below the limit"
    );
    let landed = wf_workflow::message_context::get_context(&variables, "chat");
    assert!(
        landed.iter().any(|message| message.content
            == MessageContentValue::Text("compressed summary".to_string())),
        "the compressed summary must land in the registry array"
    );

    contexts.unregister(&execution_id);
    stop_trigger_listener(listener).await;
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
                max_output_tokens: None,
                proxy: None,
                no_proxy: None,
                circuit_breaker: None,
                prompt_caching: None,
                auth: None,
                compat: None,
                identity: None,
                transport: None,
                env: None,
            })
            .expect("test profile registers");
    }

    let contexts = Arc::new(ExecutionContextRegistry::new());
    let hook_handler_registry = Arc::new(HookHandlerRegistry::new());
    // No summary runner is wired here: the short array never emits, so no
    // listener route is armed and the adapter registration is inert.
    register_compression_handler(
        &hook_handler_registry,
        CompressionHandlerDeps {
            event_bus: bus.clone(),
            pipeline: None,
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
    let entity =
        WorkflowExecutionEntity::new(exec_ctx.execution_id.clone(), exec_ctx.workflow_id.clone());
    let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
        .unwrap()
        .with_entity(entity);
    assert!(coordinator.execute().await.is_ok());

    // No compression request within the observation window.
    let compression_observed = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            match sub.recv().await {
                Ok(event) if event.r#type == EventType::ContextCompressionRequested => return true,
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
