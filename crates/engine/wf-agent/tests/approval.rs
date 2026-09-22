//! Approval gate integration tests (`approval` + `coordinator::tool`):
//! the human approval handler is consulted end-to-end through the agent
//! loop, denials surface as tool messages without executing the tool.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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
    }
}

fn registry_with_counted_echo(counter: Arc<AtomicUsize>) -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
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
    registry
}

fn gateway_with(mock: Arc<MockLlmClient>) -> Arc<LlmGateway> {
    let gateway = LlmGateway::new();
    gateway.register_mock("mock", mock);
    Arc::new(gateway)
}

fn config() -> AgentLoopConfig {
    AgentLoopConfig {
        agent_id: "approval-agent".to_string(),
        model: "mock".to_string(),
        max_iterations: Some(5),
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

struct RejectingHandler {
    reason: String,
}

#[async_trait::async_trait]
impl ToolApprovalHandler for RejectingHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        ToolApprovalResult::rejected(request.tool_call_id.clone(), self.reason.clone())
    }
}

struct ApprovingHandler;

#[async_trait::async_trait]
impl ToolApprovalHandler for ApprovingHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        ToolApprovalResult::approved(request.tool_call_id.clone())
    }
}

struct EditingHandler {
    edited: serde_json::Value,
}

#[async_trait::async_trait]
impl ToolApprovalHandler for EditingHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        ToolApprovalResult {
            tool_call_id: request.tool_call_id.clone(),
            approved: true,
            edited_parameters: Some(self.edited.clone()),
            user_instruction: None,
            rejection_reason: None,
        }
    }
}

#[tokio::test]
async fn default_without_handler_auto_approves_and_executes() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"hi"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator =
        AgentLoopCoordinator::new(gateway_with(mock), registry_with_counted_echo(counter.clone()));
    let output = coordinator.execute(config(), input("run")).await.unwrap();
    assert_eq!(output.result, serde_json::json!("done"));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn rejecting_handler_blocks_tool_without_execution() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"hi"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(
        gateway_with(mock),
        registry_with_counted_echo(counter.clone()),
    )
    .with_approval_handler(Arc::new(RejectingHandler {
        reason: "too risky".to_string(),
    }));
    let output = coordinator.execute(config(), input("run")).await.unwrap();
    assert_eq!(output.result, serde_json::json!("done"));
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "rejected tool must never execute"
    );
    let texts = tool_texts(&output.conversation);
    assert_eq!(texts.len(), 1);
    assert!(
        texts[0].contains("too risky"),
        "rejection reason surfaces in the tool message: {}",
        texts[0]
    );
    assert!(texts[0].contains("echo"), "tool name surfaces: {}", texts[0]);
}

#[tokio::test]
async fn approving_handler_allows_execution() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"hi"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(
        gateway_with(mock),
        registry_with_counted_echo(counter.clone()),
    )
    .with_approval_handler(Arc::new(ApprovingHandler));
    let output = coordinator.execute(config(), input("run")).await.unwrap();
    assert_eq!(output.result, serde_json::json!("done"));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn approval_edited_parameters_reach_the_tool() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"original"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let coordinator = AgentLoopCoordinator::new(
        gateway_with(mock),
        registry_with_counted_echo(counter.clone()),
    )
    .with_approval_handler(Arc::new(EditingHandler {
        edited: serde_json::json!({"text": "edited"}),
    }));
    let output = coordinator.execute(config(), input("run")).await.unwrap();
    assert_eq!(output.result, serde_json::json!("done"));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    let texts = tool_texts(&output.conversation);
    assert_eq!(texts.len(), 1);
    assert!(
        texts[0].contains("edited"),
        "edited parameters must reach the tool: {}",
        texts[0]
    );
    assert!(
        !texts[0].contains("original"),
        "original parameters must be replaced: {}",
        texts[0]
    );
}
