//! Core agent loop integration tests (coordinator lifecycle + execution +
//! iteration): tool-loop convergence, error propagation and iteration cap.
//!
//! Covers `coordinator::{lifecycle,execution,iteration}` through the public
//! `AgentLoopCoordinator` API with the scriptable mock LLM provider.

use std::sync::Arc;

use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
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
        tool_call_protocol: None,
        token_limit: None,
        token_warning_threshold: None,
        enable_token_tracking: None,
        general_description: None,
        discoverable_metadata_block: None,
        history_normalization: false,
        checkpoint_message_interval: None,
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

    let second = &mock.recorded_requests()[1];
    assert!(second
        .messages
        .iter()
        .any(|m| { m.role == MessageRole::Tool && m.tool_call_id.as_deref() == Some("call_1") }));

    assert_eq!(output.conversation.len(), 4);
}

#[tokio::test]
async fn llm_error_fails_the_agent_loop() {
    use wf_llm::LlmError;

    let mock = Arc::new(MockLlmClient::new());
    mock.script_error(LlmError::AuthError("invalid key".to_string()));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry_with_echo());
    let err = coordinator
        .execute(config(3), input("do it"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("invalid key"));
}

#[tokio::test]
async fn max_iterations_limits_mock_driven_loops() {
    let mock = Arc::new(MockLlmClient::new());
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
