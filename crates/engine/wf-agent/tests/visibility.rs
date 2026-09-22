//! Visibility gate integration tests (`visibility` +
//! `coordinator::tool` execution interception): blocked tools are rejected
//! at execution time without touching the visible schema.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_agent::coordinator::tool::ToolVisibilityStore;
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
        agent_id: "visibility-agent".to_string(),
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

struct BlockStore {
    blocked: HashSet<String>,
}

#[async_trait::async_trait]
impl ToolVisibilityStore for BlockStore {
    async fn is_tool_visible(&self, _execution_id: &str, tool_name: &str) -> bool {
        !self.blocked.contains(tool_name)
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
async fn blocked_tool_is_rejected_without_execution() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"hi"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let store = Arc::new(BlockStore {
        blocked: HashSet::from(["echo".to_string()]),
    });
    let coordinator = AgentLoopCoordinator::new(
        gateway_with(mock),
        registry_with_counted_echo(counter.clone()),
    )
    .with_visibility_store(store);
    let output = coordinator.execute(config(), input("run")).await.unwrap();
    assert_eq!(output.result, serde_json::json!("done"));
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "blocked tool must never execute"
    );
    let texts = tool_texts(&output.conversation);
    assert_eq!(texts.len(), 1);
    assert!(
        texts[0].contains("not visible") || texts[0].contains("unavailable"),
        "visibility denial surfaces in the tool message: {}",
        texts[0]
    );
}

#[tokio::test]
async fn unblocked_tool_executes_normally() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"hi"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let store = Arc::new(BlockStore {
        blocked: HashSet::new(),
    });
    let coordinator = AgentLoopCoordinator::new(
        gateway_with(mock),
        registry_with_counted_echo(counter.clone()),
    )
    .with_visibility_store(store);
    let output = coordinator.execute(config(), input("run")).await.unwrap();
    assert_eq!(output.result, serde_json::json!("done"));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn variable_backed_store_blocks_marked_tools() {
    use dashmap::DashMap;
    use wf_agent::visibility::{VariableBackedVisibilityStore, BLOCKED_VARIABLE_PREFIX};

    let variables = Arc::new(DashMap::new());
    variables.insert(
        format!("{BLOCKED_VARIABLE_PREFIX}echo"),
        serde_json::Value::Bool(true),
    );
    let store = VariableBackedVisibilityStore::new(variables);
    assert!(!store.is_tool_visible("exec-1", "echo").await);
    assert!(store.is_tool_visible("exec-1", "other").await);

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
    .with_visibility_store(Arc::new(store));
    let output = coordinator.execute(config(), input("run")).await.unwrap();
    assert_eq!(output.result, serde_json::json!("done"));
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}
