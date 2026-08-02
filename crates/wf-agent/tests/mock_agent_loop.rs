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
        tool_call_format: None,
        token_limit: None,
        token_warning_threshold: None,
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
    config.token_warning_threshold = Some(70); // 120/150 = 80% > 70

    let output = coordinator.execute(config, input("do it")).await.unwrap();
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
    entity.state.write().await.start();

    let coordinator = wf_agent::coordinator::iteration::AgentIterationCoordinator::new(
        gateway_with(mock.clone()),
        Arc::new(ToolRegistry::new()),
        Arc::new(wf_execution_shared::hooks::executor::HookExecutor::new()),
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
