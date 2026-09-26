//! Streaming execution integration tests (`stream::AgentEventStream` +
//! iteration streaming mode): completed outcome, tool lifecycle events and
//! failure propagation.

use std::sync::Arc;

use futures::StreamExt;
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_llm::{LlmError, LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_tools::registry::ToolRegistry;
use wf_types::message::{LlmFunctionCall, LlmToolCall};

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
async fn stream_tool_loop_emits_iteration_and_tool_lifecycle() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"hi"}"#,
    )]));
    mock.script(LlmResponseSpec::text("streamed final"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry_with_echo());
    let mut stream = coordinator.execute_stream(config(5), input("do it")).await;

    let mut saw_iteration_start = false;
    let mut saw_tool_start = false;
    let mut saw_tool_end = false;
    let mut saw_iteration_end = false;
    let mut completed = None;
    while let Some(event) = stream.next().await {
        match &event {
            wf_agent::AgentStreamEvent::IterationStart { iteration, .. } => {
                assert!(*iteration >= 1);
                saw_iteration_start = true;
            }
            wf_agent::AgentStreamEvent::ToolStart {
                tool_call_id,
                tool_name,
            } => {
                assert_eq!(tool_call_id, "call_1");
                assert_eq!(tool_name, "echo");
                saw_tool_start = true;
            }
            wf_agent::AgentStreamEvent::ToolEnd {
                tool_call_id,
                tool_name,
                success,
                ..
            } => {
                assert_eq!(tool_call_id, "call_1");
                assert_eq!(tool_name, "echo");
                assert!(*success);
                saw_tool_end = true;
            }
            wf_agent::AgentStreamEvent::IterationEnd { iteration, .. } => {
                assert!(*iteration >= 1);
                saw_iteration_end = true;
            }
            wf_agent::AgentStreamEvent::Completed { result, .. } => {
                completed = Some(result.clone());
            }
            _ => {}
        }
    }
    assert!(saw_iteration_start, "IterationStart must be emitted");
    assert!(saw_tool_start, "ToolStart must be emitted");
    assert!(saw_tool_end, "ToolEnd must be emitted");
    assert!(saw_iteration_end, "IterationEnd must be emitted");
    assert_eq!(completed, Some(serde_json::json!("streamed final")));
}

#[tokio::test]
async fn stream_failure_emits_failed_event() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script_error(LlmError::AuthError("bad key".to_string()));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry_with_echo());
    let mut stream = coordinator.execute_stream(config(3), input("do it")).await;

    let mut failed = None;
    while let Some(event) = stream.next().await {
        if let wf_agent::AgentStreamEvent::Failed { error, .. } = &event {
            failed = Some(error.clone());
        }
    }
    let failed = failed.expect("Failed event must be emitted");
    assert!(
        failed.contains("bad key"),
        "failure must surface the LLM error: {failed}"
    );
}
