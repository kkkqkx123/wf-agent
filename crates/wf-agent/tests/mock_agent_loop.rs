//! Agent loop tests driven by the scriptable mock LLM provider
//! (wf-llm `mock` feature): tool-loop convergence, error propagation and
//! stream execution.

use std::sync::Arc;

use futures::StreamExt;
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_llm::{LlmError, LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_tools::registry::ToolRegistry;
use wf_types::message::{LlmFunctionCall, LlmToolCall, Message, MessageContentValue, MessageRole};

fn tool_call(id: &str, name: &str, args: &str) -> LlmToolCall {
    LlmToolCall {
        id: id.to_string(),
        r#type: "function".to_string(),
        function: LlmFunctionCall {
            name: name.to_string(),
            arguments: args.to_string(),
        },
    }
}

fn registry_with_echo() -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
    registry.register_stateless_handler(
        "echo",
        Arc::new(|params, _ctx| {
            Ok(serde_json::json!({
                "echoed": params.get("text").cloned().unwrap_or(serde_json::Value::Null)
            }))
        }),
    );
    registry.register_tool(wf_types::tool::Tool {
        id: "echo".to_string(),
        name: "echo".to_string(),
        description: "Echo the given text back".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: None,
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: None,
    });
    registry
}

fn gateway_with(mock: Arc<MockLlmClient>) -> Arc<LlmGateway> {
    let gateway = LlmGateway::new();
    gateway.register_mock("mock", mock);
    Arc::new(gateway)
}

fn config(max_iterations: u32) -> AgentLoopConfig {
    AgentLoopConfig {
        agent_id: "agent1".to_string(),
        model: "mock".to_string(),
        max_iterations: Some(max_iterations),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: vec!["echo".to_string()],
        initial_tool_names: Vec::new(),
        discoverable_tool_names: Vec::new(),
        enable_general_tool: None,
        activated_tool_names: Vec::new(),
        hidden_tool_names: Vec::new(),
        tool_call_format: None,
        token_limit: None,
        token_warning_threshold: None,
        enable_token_tracking: None,
        general_description: None,
        discoverable_metadata_block: None,
    }
}

fn input(message: &str) -> AgentLoopInput {
    AgentLoopInput {
        message: message.to_string(),
        context: std::collections::HashMap::new(),
        conversation: Vec::new(),
    }
}

#[tokio::test]
async fn tool_loop_converges_with_mock_script() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"agent ping"}"#,
    )]));
    mock.script(LlmResponseSpec::text("final answer"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), registry_with_echo());
    let output = coordinator
        .execute(config(5), input("do it"))
        .await
        .unwrap();

    assert_eq!(output.result, serde_json::json!("final answer"));
    assert_eq!(output.iterations, 2);
    assert_eq!(mock.recorded_count(), 2);

    // The second request must carry the tool result back.
    let second = &mock.recorded_requests()[1];
    assert!(second
        .messages
        .iter()
        .any(|m| { m.role == MessageRole::Tool && m.tool_call_id.as_deref() == Some("call_1") }));

    // The exported conversation contains user + assistant(tool) + tool result
    // + assistant(final).
    assert_eq!(output.conversation.len(), 4);
}

#[tokio::test]
async fn llm_error_fails_the_agent_loop() {
    let mock = Arc::new(MockLlmClient::new());
    // AuthError is classified as non-retryable by the agent failure policy.
    mock.script_error(LlmError::AuthError("invalid key".to_string()));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry_with_echo());
    let err = coordinator
        .execute(config(3), input("do it"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("invalid key"));
}

#[tokio::test]
async fn stream_execution_emits_completed_event() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("streamed final"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry_with_echo());
    let mut stream = coordinator.execute_stream(config(3), input("do it")).await;

    let mut completed = None;
    while let Some(event) = stream.next().await {
        if let wf_agent::AgentStreamEvent::Completed { result, .. } = &event {
            completed = Some(result.clone());
        }
    }
    assert_eq!(completed, Some(serde_json::json!("streamed final")));
}

#[tokio::test]
async fn max_iterations_limits_mock_driven_loops() {
    let mock = Arc::new(MockLlmClient::new());
    // Always ask for a tool call: the loop must stop at max_iterations.
    mock.default(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"loop"}"#,
    )]));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), registry_with_echo());
    let output = coordinator
        .execute(config(3), input("loop forever"))
        .await
        .unwrap();
    assert_eq!(output.iterations, 3);
    assert_eq!(mock.recorded_count(), 3);
}

// ── general tool discovery flow (§6.3 of the general-tool plan) ────────

/// Registry with the `general` builtin tool plus web_search (discoverable)
/// and write_file (gated) stateless tools.
fn discovery_registry() -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
    registry.register_tool(wf_tools::predefined::general::GENERAL.tool_def());
    registry.register_tool(wf_tools::predefined::web::WEB_SEARCH.tool_def());
    registry.register_tool(wf_tools::predefined::filesystem::WRITE_FILE.tool_def());

    registry.register_stateless_handler(
        "web_search",
        Arc::new(|params, _ctx| {
            Ok(serde_json::json!({
                "results": [params.get("query").cloned().unwrap_or(serde_json::Value::Null)]
            }))
        }),
    );
    registry.register_stateless_handler(
        "write_file",
        Arc::new(|_params, _ctx| Ok(serde_json::json!({ "written": true }))),
    );
    registry
}

fn discovery_config(activated: Vec<String>) -> AgentLoopConfig {
    discovery_config_with_general(activated, None)
}

fn discovery_config_with_general(
    activated: Vec<String>,
    general_description: Option<String>,
) -> AgentLoopConfig {
    AgentLoopConfig {
        agent_id: "discovery-agent".to_string(),
        model: "mock".to_string(),
        max_iterations: Some(5),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: vec!["web_search".to_string(), "write_file".to_string()],
        // Non-empty initial: write_file stays gated (an empty initial list
        // would make every available tool visible, per the exposure rules).
        initial_tool_names: vec!["web_search".to_string()],
        discoverable_tool_names: vec!["web_search".to_string()],
        enable_general_tool: None,
        activated_tool_names: activated,
        hidden_tool_names: Vec::new(),
        tool_call_format: None,
        token_limit: None,
        token_warning_threshold: None,
        enable_token_tracking: None,
        general_description,
        discoverable_metadata_block: None,
    }
}

/// Tool names rendered into the recorded request's `tools` schema.
fn request_tool_names(request: &wf_types::llm::LlmRequest) -> Vec<String> {
    request
        .tools
        .as_ref()
        .map(|tools| tools.iter().map(|t| t.name.clone()).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn discoverable_tool_invoked_via_general_without_schema_injection() {
    let mock = Arc::new(MockLlmClient::new());
    // Turn 1: the model reaches web_search through the general tool.
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "general",
        r#"{"request":"{\"tool\": \"web_search\", \"parameters\": {\"query\": \"rust 异步\"}}"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), discovery_registry());
    let output = coordinator
        .execute(discovery_config(Vec::new()), input("search"))
        .await
        .unwrap();
    assert_eq!(output.result, serde_json::json!("done"));

    // The schema stays stable: web_search is NOT injected, general is.
    let first = &mock.recorded_requests()[0];
    let names = request_tool_names(first);
    assert!(
        names.contains(&"general".to_string()),
        "general must be in the visible schema: {names:?}"
    );
    assert!(
        !names.contains(&"web_search".to_string()),
        "discoverable web_search must not enter the schema: {names:?}"
    );

    // The tool result carried the inner tool's native output.
    let tool_msg = output
        .conversation
        .iter()
        .find(|m| m.role == MessageRole::Tool && m.tool_name.as_deref() == Some("general"))
        .expect("general tool result must be in the conversation");
    let content = match &tool_msg.content {
        MessageContentValue::Text(t) => t.clone(),
        _ => String::new(),
    };
    assert!(
        content.contains("rust 异步"),
        "general must return the inner tool's native result: {content}"
    );
}

#[tokio::test]
async fn general_description_override_reaches_the_schema() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), discovery_registry());
    let custom = "Custom general description: format=xml".to_string();
    let output = coordinator
        .execute(
            discovery_config_with_general(Vec::new(), Some(custom.clone())),
            input("search"),
        )
        .await
        .unwrap();
    assert_eq!(output.result, serde_json::json!("done"));

    // The routed `general` tool copy carries the rendered description while
    // the shared registry's tool is untouched.
    let first = &mock.recorded_requests()[0];
    let general = first
        .tools
        .as_ref()
        .expect("tools schema present")
        .iter()
        .find(|t| t.name == "general")
        .expect("general in visible schema");
    assert_eq!(general.description, custom);

    let registry_tool = discovery_registry()
        .list_tools()
        .into_iter()
        .find(|t| t.name == "general")
        .expect("general in registry");
    assert_ne!(
        registry_tool.description, custom,
        "registry must stay untouched"
    );
}

#[tokio::test]
async fn gated_tool_rejected_via_general_until_activated() {
    let mock = Arc::new(MockLlmClient::new());
    // write_file is gated (not initial, not discoverable, not activated).
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "general",
        r#"{"request":"{\"tool\": \"write_file\", \"parameters\": {\"path\": \"a.txt\"}}"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), discovery_registry());
    let output = coordinator
        .execute(discovery_config(Vec::new()), input("write it"))
        .await
        .unwrap();

    let tool_msg = output
        .conversation
        .iter()
        .find(|m| m.role == MessageRole::Tool && m.tool_name.as_deref() == Some("general"))
        .expect("rejection must surface as a tool message");
    let content = match &tool_msg.content {
        MessageContentValue::Text(t) => t.clone(),
        _ => String::new(),
    };
    assert!(
        content.contains("not activated"),
        "gated tool must be rejected with a guidance error: {content}"
    );
}

#[tokio::test]
async fn activated_gated_tool_enters_schema_and_is_callable() {
    let mock = Arc::new(MockLlmClient::new());
    // With write_file formally activated, the model calls it directly.
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "write_file",
        r#"{"path":"a.txt","content":"hi"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), discovery_registry());
    let output = coordinator
        .execute(
            discovery_config(vec!["write_file".to_string()]),
            input("write it"),
        )
        .await
        .unwrap();
    assert_eq!(output.result, serde_json::json!("done"));

    let first = &mock.recorded_requests()[0];
    let names = request_tool_names(first);
    assert!(
        names.contains(&"write_file".to_string()),
        "activated gated tool must enter the visible schema: {names:?}"
    );
    assert!(
        !names.contains(&"web_search".to_string()),
        "discoverable web_search still not in schema: {names:?}"
    );
}

#[tokio::test]
async fn unactivated_loop_keeps_prefix_stable_across_turns() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "general",
        r#"{"request":"{\"tool\": \"web_search\", \"parameters\": {\"query\": \"x\"}}"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), discovery_registry());
    let output = coordinator
        .execute(discovery_config(Vec::new()), input("search"))
        .await
        .unwrap();
    assert_eq!(output.result, serde_json::json!("done"));

    // No activation happened: the tool schema is byte-identical across turns
    // (system prompt prefix stability, KV-cache friendly).
    let requests = mock.recorded_requests();
    assert_eq!(requests.len(), 2);
    let first = request_tool_names(&requests[0]);
    let second = request_tool_names(&requests[1]);
    assert_eq!(first, second, "schema must not change between turns");
}

#[tokio::test]
async fn token_events_emitted_when_limit_crossed() {
    let mock = Arc::new(MockLlmClient::new());
    // First call: tool round (120 tokens), second call: final answer (120 tokens).
    mock.script(
        LlmResponseSpec::tool_calls(vec![tool_call(
            "call_1",
            "echo",
            r#"{"text":"agent ping"}"#,
        )])
        .with_usage(100, 20),
    );
    mock.script(LlmResponseSpec::text("final answer").with_usage(100, 20));

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), registry_with_echo())
        .with_event_bus(bus);
    let mut config = config(5);
    config.token_limit = Some(150);
    config.token_warning_threshold = Some(70);
    // The decision track is estimation-only: a long request makes the local
    // estimate cross the limit regardless of the provider-reported usage.
    let long_input = input(&"x".repeat(4000));

    let output = coordinator.execute(config, long_input).await.unwrap();
    assert_eq!(output.result, serde_json::json!("final answer"));
    // The pre-flight check must not block over-budget requests.
    assert_eq!(mock.recorded_count(), 2, "requests must still be executed");

    let mut saw_warning = false;
    let mut saw_limit = false;
    let mut saw_compression = false;
    let mut warning_index: Option<usize> = None;
    let mut limit_index: Option<usize> = None;
    let mut index = 0usize;
    while let Ok(event) = sub.try_recv() {
        match event.r#type {
            wf_types::events::EventType::TokenUsageWarning => {
                saw_warning = true;
                warning_index = Some(index);
            }
            wf_types::events::EventType::TokenLimitExceeded => {
                saw_limit = true;
                limit_index = Some(index);
            }
            wf_types::events::EventType::ContextCompressionRequested => saw_compression = true,
            _ => {}
        }
        index += 1;
    }

    // 240 cumulative > 150 limit -> warning (80% > 70%) + limit + compression
    assert!(saw_warning, "TokenUsageWarning must be emitted");
    assert!(saw_limit, "TokenLimitExceeded must be emitted");
    assert!(
        saw_compression,
        "ContextCompressionRequested must be emitted"
    );
    assert!(
        warning_index < limit_index,
        "TokenUsageWarning must be emitted before TokenLimitExceeded"
    );
}

#[tokio::test]
async fn estimated_usage_recorded_when_provider_reports_none() {
    let mock = Arc::new(MockLlmClient::new());
    // LlmResponseSpec::text carries usage = None by default: the tracker must
    // fall back to estimation so limit checks and warnings still work.
    mock.script(LlmResponseSpec::text("final answer"));

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let entity =
        wf_agent::entity::AgentLoopEntity::new(wf_types::Id::from("agent-est-1".to_string()))
            .with_model("mock".to_string());
    {
        let mut conversation = entity.conversation().write().await;
        conversation.set_token_limit(5);
        conversation.add_message(Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text("hello world".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        });
    }
    entity.state.write().await.start().unwrap();

    let coordinator = wf_agent::coordinator::iteration::AgentIterationCoordinator::new(
        gateway_with(mock.clone()),
        Arc::new(ToolRegistry::new()),
        None,
    )
    .with_event_bus(bus)
    .with_token_warning_threshold(70);

    let result = coordinator
        .execute_iteration(&entity)
        .await
        .expect("iteration must succeed");
    assert_eq!(result.content, serde_json::json!("final answer"));

    let conversation = entity.conversation().read().await;
    assert!(
        conversation.token_usage() > 0,
        "estimated usage must be recorded when the provider reports none"
    );
    assert!(
        conversation.is_token_limit_exceeded(),
        "estimated usage must feed limit checks"
    );
    drop(conversation);

    let mut saw_warning = false;
    while let Ok(event) = sub.try_recv() {
        if event.r#type == wf_types::events::EventType::TokenUsageWarning {
            saw_warning = true;
        }
    }
    assert!(saw_warning, "TokenUsageWarning must be emitted");
}

#[tokio::test]
async fn token_tracking_disabled_suppresses_usage_and_events() {
    let mock = Arc::new(MockLlmClient::new());
    mock.default(LlmResponseSpec::text("final answer").with_usage(100, 20));

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), registry_with_echo())
        .with_event_bus(bus);
    let mut config = config(2);
    config.token_limit = Some(10);
    config.token_warning_threshold = Some(50);
    config.enable_token_tracking = Some(false);

    let output = coordinator.execute(config, input("do it")).await.unwrap();
    assert_eq!(output.result, serde_json::json!("final answer"));

    // Explicit disable: token events must not be emitted even though the
    // usage (100+20 per call) far exceeds the configured limit of 10.
    let mut saw_token_event = false;
    while let Ok(event) = sub.try_recv() {
        if matches!(
            event.r#type,
            wf_types::events::EventType::TokenUsageWarning
                | wf_types::events::EventType::TokenLimitExceeded
                | wf_types::events::EventType::ContextCompressionRequested
        ) {
            saw_token_event = true;
        }
    }
    assert!(
        !saw_token_event,
        "no token events when enable_token_tracking = false"
    );
}

// ── checkpoint restore / resume (P1-1) ────────────────────────────────

/// A run interrupted mid-way checkpoints its progress; a resumed run rebuilds
/// the entity from the checkpoint and re-drives the loop. A tool call id that
/// already produced a result is served from the replay cache without invoking
/// the tool again.
#[tokio::test]
async fn resume_from_checkpoint_replays_idempotent_tool_calls() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use wf_agent::AgentCheckpointStrategy;
    use wf_checkpoint::state::agent::AgentCheckpointStateManager;
    use wf_checkpoint::state::CheckpointStateManager;
    use wf_storage::backend::StorageBackend;
    use wf_types::Id;

    // Registry whose echo handler counts actual executions: idempotent replay
    // must not increment it a second time.
    let registry = Arc::new(ToolRegistry::new());
    let echo_runs = Arc::new(AtomicUsize::new(0));
    let counter = echo_runs.clone();
    registry.register_stateless_handler(
        "echo",
        Arc::new(move |params, _ctx| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({
                "echoed": params.get("text").cloned().unwrap_or(serde_json::Value::Null)
            }))
        }),
    );
    registry.register_tool(wf_types::tool::Tool {
        id: "echo".to_string(),
        name: "echo".to_string(),
        description: "Echo the given text back".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: None,
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: None,
    });

    let store = Arc::new(StorageBackend::new_memory());
    let mock = Arc::new(MockLlmClient::new());
    // First run: iteration 1 executes `call_1`, iteration 2 hits a
    // non-retryable error that interrupts the loop.
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"ping"}"#,
    )]));
    mock.script_error(LlmError::AuthError("interrupted".to_string()));

    let coordinator = AgentLoopCoordinator::with_store(
        gateway_with(mock.clone()),
        registry.clone(),
        store.clone(),
    )
    .with_agent_loop_id(Id::from("restore-loop"))
    .with_checkpoint_strategy(AgentCheckpointStrategy::every_iteration());

    let err = coordinator
        .execute(config(5), input("first run"))
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("interrupted"),
        "first run must be interrupted: {err}"
    );
    assert_eq!(echo_runs.load(Ordering::SeqCst), 1, "echo executed once");

    // The interruption checkpoints (start / after iteration 1 / on error);
    // resume from the latest one.
    let sm = AgentCheckpointStateManager::new(store.clone());
    let meta = sm
        .get_latest("restore-loop")
        .await
        .unwrap()
        .expect("interrupted run left a checkpoint");

    // Resumed run: the LLM re-issues the same tool call id, then answers.
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"ping"}"#,
    )]));
    mock.script(LlmResponseSpec::text("recovered"));

    let output = coordinator
        .resume_from_checkpoint(&meta.id, config(5), input("continue"))
        .await
        .unwrap();
    assert_eq!(output.result, serde_json::json!("recovered"));
    assert!(
        output.iterations >= 2,
        "resumed run continues beyond the restored iteration count"
    );
    assert_eq!(
        echo_runs.load(Ordering::SeqCst),
        1,
        "replayed tool call served from the idempotency cache, not re-executed"
    );
}
