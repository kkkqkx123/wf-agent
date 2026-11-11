use super::*;

#[tokio::test]
async fn test_execution_callback_wired_call_agent_via_tool() {
    clear_env_vars();

    let config = default_test_config();
    let runtime = Runtime::bootstrap(config).await.unwrap();

    register_agent_tools(&runtime);
    mock_llm_gateway(&runtime, "mock", "agent answer");

    let (ctx, options) = dispatch_tool_context();

    // call_agent through the shared tool registry hits the composite
    // callback (previously CallbackNotRegistered in production).
    let result = runtime
        .tool_registry()
        .execute_tool(
            "call_agent",
            &serde_json::json!({
                "agent_id": "integration-agent",
                "agent_profile_id": "mock",
                "prompt": "hello",
            }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert!(result.success, "call_agent failed: {:?}", result.error);
    assert_eq!(result.result.unwrap()["result"], "agent answer");

    // The execution is registered in the shared agent loop registry and
    // observable through the query_workflow_status tool with its result.
    let ids = runtime.api_context().agent_loops.get_all_ids();
    assert_eq!(ids.len(), 1, "execution must be registered");
    let status = query_status_via_tool(&runtime, &ids[0].to_string()).await;
    assert_eq!(status["status"], "completed");
    assert_eq!(status["result"], "agent answer");

    runtime.shutdown().await.unwrap();
    clear_env_vars();
}

#[tokio::test]
async fn test_execution_callback_call_agent_wait_false_spawns() {
    clear_env_vars();

    let config = default_test_config();
    let runtime = Runtime::bootstrap(config).await.unwrap();

    register_agent_tools(&runtime);
    mock_llm_gateway(&runtime, "mock", "async answer");

    let (ctx, options) = dispatch_tool_context();

    let result = runtime
        .tool_registry()
        .execute_tool(
            "call_agent",
            &serde_json::json!({
                "agent_id": "integration-agent",
                "agent_profile_id": "mock",
                "prompt": "hello",
                "wait": false,
            }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert!(
        result.success,
        "spawned call_agent failed: {:?}",
        result.error
    );
    let value = result.result.unwrap();
    assert_eq!(value["status"], "started");
    let execution_id = value["execution_id"].as_str().unwrap().to_string();

    // The spawned execution progresses in the background; polling the
    // query tool eventually returns the result.
    let status = poll_until_completed(&runtime, &execution_id).await;
    assert_eq!(status["result"], "async answer");

    runtime.shutdown().await.unwrap();
    clear_env_vars();
}

#[tokio::test]
async fn test_execution_callback_execute_workflow_via_tool() {
    clear_env_vars();

    let config = default_test_config();
    let runtime = Runtime::bootstrap(config).await.unwrap();

    register_workflow_tools(&runtime);

    // The @standard/fold-summary LLM node uses the DEFAULT profile.
    let mock = Arc::new(wf_llm::mock::MockLlmClient::new());
    mock.default(wf_llm::mock::LlmResponseSpec::text("compressed").with_usage(50, 30));
    runtime.llm_gateway().register_mock("DEFAULT", mock);

    let (ctx, options) = dispatch_tool_context();

    let message = wf_types::message::Message {
        id: wf_common::generate_id(),
        role: wf_types::message::MessageRole::User,
        content: wf_types::message::MessageContentValue::Text("long context".to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    let result = runtime
        .tool_registry()
        .execute_tool(
            "execute_workflow",
            &serde_json::json!({
                "workflow_id": "@standard/fold-summary",
                "input": {
                    "conversationHistory": [
                        serde_json::to_value(&message).unwrap()
                    ]
                }
            }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert!(
        result.success,
        "execute_workflow failed: {:?}",
        result.error
    );
    let value = result.result.unwrap();
    let execution_id = value["execution_id"].as_str().unwrap().to_string();

    // Registered resource workflows are resolvable; the execution
    // completes and its status is queryable through the query tool.
    assert!(
        wf_common::poll_until(
            std::time::Duration::from_millis(10),
            std::time::Duration::from_secs(2),
            || async {
                let status = query_status_via_tool(&runtime, &execution_id).await;
                let state = status["status"].as_str().unwrap_or_default().to_string();
                state == "completed" || state == "failed"
            },
        )
        .await,
        "workflow execution did not settle"
    );
    let status = query_status_via_tool(&runtime, &execution_id).await;
    assert_eq!(
        status["status"].as_str().unwrap_or_default(),
        "completed",
        "status: {status:?}"
    );

    runtime.shutdown().await.unwrap();
    clear_env_vars();
}
