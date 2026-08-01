//! Agent loop tests driven by the scriptable mock LLM provider
//! (wf-llm `mock` feature): tool-loop convergence, error propagation and
//! stream execution.

use std::sync::Arc;

use futures::StreamExt;
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_llm::{ClientFactory, LlmError, LlmResponseSpec, LlmWrapper, MockLlmClient};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_tools::registry::ToolRegistry;
use wf_types::message::{LlmFunctionCall, LlmToolCall, MessageRole};

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

fn wrapper_with(mock: Arc<MockLlmClient>) -> Arc<LlmWrapper> {
    let factory = ClientFactory::new();
    factory.register_mock("mock", mock);
    Arc::new(LlmWrapper::with_factory(factory))
}

fn config(max_iterations: u32) -> AgentLoopConfig {
    AgentLoopConfig {
        agent_id: "agent1".to_string(),
        model: Some("mock".to_string()),
        max_iterations: Some(max_iterations),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: vec!["echo".to_string()],
        tool_call_format: None,
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

    let coordinator = AgentLoopCoordinator::new(wrapper_with(mock.clone()), registry_with_echo());
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

    let coordinator = AgentLoopCoordinator::new(wrapper_with(mock), registry_with_echo());
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

    let coordinator = AgentLoopCoordinator::new(wrapper_with(mock), registry_with_echo());
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

    let coordinator = AgentLoopCoordinator::new(wrapper_with(mock.clone()), registry_with_echo());
    let output = coordinator
        .execute(config(3), input("loop forever"))
        .await
        .unwrap();
    assert_eq!(output.iterations, 3);
    assert_eq!(mock.recorded_count(), 3);
}
