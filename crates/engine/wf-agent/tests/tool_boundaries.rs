//! Tool execution boundary tests (`coordinator::tool` through the public
//! `AgentLoopCoordinator`): approval rejection, handler failures, tool
//! timeouts, permanently-failed reporting and streaming/blocking parity.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use wf_agent::approval::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_tools::registry::ToolRegistry;
use wf_types::message::{LlmFunctionCall, LlmToolCall, MessageContentValue, MessageRole};

fn tool_call(id: &str, name: &str, args: &str) -> LlmToolCall {
    LlmToolCall {
        id: id.to_string(),
        r#type: "function".to_string(),
        function: LlmFunctionCall {
            name: name.to_string(),
            arguments: args.to_string(),
        },
        call_id: None,
        thought_signature: None,
    }
}

fn tool_def(name: &str, timeout_ms: Option<u64>) -> wf_types::tool::Tool {
    wf_types::tool::Tool {
        id: name.to_string(),
        name: name.to_string(),
        description: format!("{name} tool"),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: None,
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: timeout_ms,
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
    registry.register_tool(tool_def("echo", None));
    registry
}

fn gateway_with(mock: Arc<MockLlmClient>) -> Arc<LlmGateway> {
    let gateway = LlmGateway::new();
    gateway.register_mock("mock", mock);
    Arc::new(gateway)
}

fn config(tool: &str) -> AgentLoopConfig {
    AgentLoopConfig {
        file_scope: None,
        agent_id: "boundary-agent".to_string(),
        model: "mock".to_string(),
        max_iterations: Some(5),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: Some(vec![tool.to_string()]),
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
        context_schema: None,
        context_strict: None,
        temporary_mode: None,
        checkpoint_message_interval: None,
    }
}

fn input(message: &str) -> AgentLoopInput {
    AgentLoopInput {
        message: message.to_string(),
        context: std::collections::HashMap::new(),
        conversation: Vec::new(),
        images: Vec::new(),
    }
}

struct RejectingHandler {
    reason: String,
}

#[async_trait::async_trait]
impl ToolApprovalHandler for RejectingHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        ToolApprovalResult::rejected(request.tool_call_id.clone(), self.reason.clone())
    }
}

fn tool_texts(conversation: &[wf_types::message::Message]) -> Vec<String> {
    conversation
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .map(|m| match &m.content {
            MessageContentValue::Text(t) => t.clone(),
            _ => String::new(),
        })
        .collect()
}

#[tokio::test]
async fn approval_rejection_reports_tool_end_failure_and_continues() {
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"hi"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry_with_echo())
        .with_approval_handler(Arc::new(RejectingHandler {
            reason: "too risky".to_string(),
        }));
    let mut stream = coordinator
        .execute_stream(config("echo"), input("run"))
        .await;

    let mut tool_end = None;
    let mut completed = None;
    while let Some(event) = stream.next().await {
        match &event {
            wf_agent::AgentStreamEvent::ToolEnd {
                tool_call_id,
                tool_name,
                success,
                error,
                ..
            } => {
                tool_end = Some((
                    tool_call_id.clone(),
                    tool_name.clone(),
                    *success,
                    error.clone(),
                ));
            }
            wf_agent::AgentStreamEvent::Completed { result, .. } => {
                completed = Some(result.clone());
            }
            _ => {}
        }
    }
    let (call_id, name, success, error) = tool_end.expect("ToolEnd must be emitted");
    assert_eq!(call_id, "call_1");
    assert_eq!(name, "echo");
    assert!(!success, "rejected calls report success=false");
    assert!(
        error.as_deref().unwrap_or_default().contains("too risky"),
        "rejection reason rides the ToolEnd error field: {error:?}"
    );
    assert_eq!(
        completed,
        Some(serde_json::json!("done")),
        "the loop continues on the rejection message"
    );
}

#[tokio::test]
async fn tool_handler_error_becomes_tool_message_without_stopping_loop() {
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
    use std::sync::atomic::{AtomicUsize, Ordering};

    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let registry = Arc::new(ToolRegistry::new());
    registry.register_stateless_handler(
        "flaky",
        Arc::new(move |_params, _ctx| {
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(wf_tools::error::ToolError::ExecutionFailed {
                    tool_id: "flaky".to_string(),
                    reason: "flaky boom".to_string(),
                });
            }
            Ok(serde_json::json!({"recovered": true}))
        }),
    );
    registry.register_tool(tool_def("flaky", None));

    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1", "flaky", r#"{}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), registry);
    let output = coordinator
        .execute(config("flaky"), input("run"))
        .await
        .unwrap();
    assert_eq!(output.result, serde_json::json!("done"));
    assert_eq!(
        mock.recorded_count(),
        2,
        "LLM decides again after the error"
    );
    let texts = tool_texts(&output.conversation);
    assert_eq!(texts.len(), 1);
    assert!(
        texts[0].contains("flaky boom"),
        "handler failure enters the tool message: {}",
        texts[0]
    );
}

#[tokio::test]
async fn tool_timeout_surfaces_as_error_message_and_continues() {
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
    let registry = Arc::new(ToolRegistry::new());
    registry.register_stateless_async_handler(
        "slow",
        Arc::new(
            |_p: serde_json::Value, _c: wf_tools::executor::trait_def::ToolExecutionContext| {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    Ok(serde_json::Value::from("too late"))
                })
            },
        ),
    );
    registry.register_tool(tool_def("slow", Some(50)));

    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1", "slow", r#"{}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), registry);
    let output = coordinator
        .execute(config("slow"), input("run"))
        .await
        .unwrap();
    assert_eq!(output.result, serde_json::json!("done"));
    assert_eq!(mock.recorded_count(), 2);
    let texts = tool_texts(&output.conversation);
    assert_eq!(texts.len(), 1);
    assert!(
        texts[0].to_lowercase().contains("timed out"),
        "timeout enters the tool message: {}",
        texts[0]
    );
}

#[tokio::test]
async fn permanently_failed_tool_listed_in_failed_event() {
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
    let registry = Arc::new(ToolRegistry::new());
    registry.register_stateless_handler(
        "broken",
        Arc::new(|_params, _ctx| {
            Err(wf_tools::error::ToolError::ExecutionFailed {
                tool_id: "broken".to_string(),
                reason: "broken pipe".to_string(),
            })
        }),
    );
    registry.register_tool(tool_def("broken", None));

    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1", "broken", r#"{}"#,
    )]));
    mock.script_error(wf_llm::LlmError::AuthError("stop here".to_string()));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry);
    let mut stream = coordinator
        .execute_stream(config("broken"), input("run"))
        .await;

    let mut failed = None;
    while let Some(event) = stream.next().await {
        if let wf_agent::AgentStreamEvent::Failed {
            error,
            permanently_failed_tools,
            ..
        } = &event
        {
            failed = Some((error.clone(), permanently_failed_tools.clone()));
        }
    }
    let (error, tools) = failed.expect("Failed event must be emitted");
    assert!(error.contains("stop here"), "root cause kept: {error}");
    assert!(
        tools.contains(&"broken".to_string()),
        "non-retryable tool failure reported: {tools:?}"
    );
}

#[tokio::test]
async fn streaming_tool_events_match_blocking_conversation() {
    wf_execution_shared::set_process_compaction_limits(
        wf_execution_shared::builtin_compaction_limits(),
    );
    fn script(mock: &MockLlmClient) {
        mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
            "call_1",
            "echo",
            r#"{"text":"hi"}"#,
        )]));
        mock.script(LlmResponseSpec::text("done"));
    }

    let blocking_mock = Arc::new(MockLlmClient::new());
    script(&blocking_mock);
    let blocking = AgentLoopCoordinator::new(gateway_with(blocking_mock), registry_with_echo());
    let blocking_output = blocking
        .execute(config("echo"), input("run"))
        .await
        .unwrap();

    let streaming_mock = Arc::new(MockLlmClient::new());
    script(&streaming_mock);
    let streaming = AgentLoopCoordinator::new(gateway_with(streaming_mock), registry_with_echo());
    let mut stream = streaming.execute_stream(config("echo"), input("run")).await;

    let mut order = Vec::new();
    let mut tool_start = None;
    let mut tool_end = None;
    let mut completed = None;
    let mut completed_iterations = None;
    while let Some(event) = stream.next().await {
        match &event {
            wf_agent::AgentStreamEvent::ToolStart {
                tool_call_id,
                tool_name,
            } => {
                order.push("start");
                tool_start = Some((tool_call_id.clone(), tool_name.clone()));
            }
            wf_agent::AgentStreamEvent::ToolEnd {
                tool_call_id,
                tool_name,
                success,
                ..
            } => {
                order.push("end");
                tool_end = Some((tool_call_id.clone(), tool_name.clone(), *success));
            }
            wf_agent::AgentStreamEvent::Completed {
                result, iterations, ..
            } => {
                order.push("completed");
                completed = Some(result.clone());
                completed_iterations = Some(*iterations);
            }
            _ => {}
        }
    }
    assert_eq!(order, vec!["start", "end", "completed"]);
    let (start_id, start_name) = tool_start.expect("ToolStart emitted");
    let (end_id, end_name, success) = tool_end.expect("ToolEnd emitted");
    assert_eq!(start_id, "call_1");
    assert_eq!((end_id, end_name), (start_id.clone(), start_name.clone()));
    assert!(success);
    assert_eq!(completed, Some(blocking_output.result.clone()));
    assert_eq!(completed_iterations, Some(blocking_output.iterations));

    let blocking_tool = blocking_output
        .conversation
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("blocking run records the tool message");
    assert_eq!(
        blocking_tool.tool_call_id.as_deref(),
        Some(start_id.as_str())
    );
    assert_eq!(
        blocking_tool.tool_name.as_deref(),
        Some(start_name.as_str())
    );
}

/// A partial failure must reach the model as the single envelope, with the
/// failed item indexes and the machine-readable metadata, and must not be
/// counted as a clean call.
#[tokio::test]
async fn partial_failure_enters_the_unified_envelope() {
    let registry = Arc::new(ToolRegistry::new());
    registry.register_stateless_handler(
        "patcher",
        Arc::new(|_params, _ctx| {
            Ok(serde_json::json!({
                "success": false,
                "error_code": "partial_failure",
                "error": "1 of 2 patch operations failed",
                "display": "Failed to update 'b.rs': could not find chunk",
                "failed_indices": [1],
            }))
        }),
    );
    registry.register_tool(tool_def("patcher", None));

    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1", "patcher", "{}",
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), registry);
    let output = coordinator
        .execute(config("patcher"), input("run"))
        .await
        .unwrap();

    let texts = tool_texts(&output.conversation);
    assert_eq!(texts.len(), 1);
    let envelope: serde_json::Value =
        serde_json::from_str(&texts[0]).expect("failure renders as JSON");
    let error = &envelope["error"];
    assert_eq!(error["code"], serde_json::json!("partial_failure"));
    assert_eq!(
        error["message"],
        serde_json::json!("1 of 2 patch operations failed")
    );
    assert_eq!(error["retryable"], serde_json::json!(false));
    assert_eq!(error["context"]["failed_indices"], serde_json::json!([1]));
    assert!(
        error["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("could not find chunk")),
        "per-item detail stays reachable: {envelope}"
    );
    // No internal engine prefix survives into the model text.
    assert!(!texts[0].contains("Tool execution failed"));
    assert!(!texts[0].contains("Execution error"));

    let tool_message = output
        .conversation
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("tool message");
    let metadata = tool_message.metadata.as_ref().expect("failure metadata");
    assert_eq!(metadata["is_error"], serde_json::json!(true));
    assert_eq!(metadata["error_code"], serde_json::json!("partial_failure"));
}
