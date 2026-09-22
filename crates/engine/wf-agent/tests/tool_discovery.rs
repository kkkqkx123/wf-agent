//! Tool discovery integration tests (`tool_router` + `agent_request` +
//! `general` tool): discoverable metadata stays out of the schema, the
//! `general` proxy routes inner calls, and formal activation gates entry.

use std::sync::Arc;

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

fn gateway_with(mock: Arc<MockLlmClient>) -> Arc<LlmGateway> {
    let gateway = LlmGateway::new();
    gateway.register_mock("mock", mock);
    Arc::new(gateway)
}

fn input(message: &str) -> AgentLoopInput {
    AgentLoopInput {
        message: message.to_string(),
        context: std::collections::HashMap::new(),
        conversation: Vec::new(),
    }
}

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
        initial_tool_names: vec!["web_search".to_string()],
        discoverable_tool_names: vec!["web_search".to_string()],
        enable_general_tool: None,
        activated_tool_names: activated,
        hidden_tool_names: Vec::new(),
        tool_call_protocol: None,
        token_limit: None,
        token_warning_threshold: None,
        enable_token_tracking: None,
        general_description,
        discoverable_metadata_block: None,
        history_normalization: false,
        checkpoint_message_interval: None,
    }
}

fn request_tool_names(request: &wf_types::llm::LlmRequest) -> Vec<String> {
    request
        .tools
        .as_ref()
        .map(|tools| tools.iter().map(|t| t.name.clone()).collect())
        .unwrap_or_default()
}

fn tool_text(msg: &wf_types::message::Message) -> String {
    match &msg.content {
        MessageContentValue::Text(t) => t.clone(),
        _ => String::new(),
    }
}

#[tokio::test]
async fn discoverable_tool_invoked_via_general_without_schema_injection() {
    let mock = Arc::new(MockLlmClient::new());
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

    let tool_msg = output
        .conversation
        .iter()
        .find(|m| m.role == MessageRole::Tool && m.tool_name.as_deref() == Some("general"))
        .expect("general tool result must be in the conversation");
    let content = tool_text(tool_msg);
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
    let content = tool_text(tool_msg);
    assert!(
        content.contains("not activated"),
        "gated tool must be rejected with a guidance error: {content}"
    );
}

#[tokio::test]
async fn activated_gated_tool_enters_schema_and_is_callable() {
    let mock = Arc::new(MockLlmClient::new());
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

    let requests = mock.recorded_requests();
    assert_eq!(requests.len(), 2);
    let first = request_tool_names(&requests[0]);
    let second = request_tool_names(&requests[1]);
    assert_eq!(first, second, "schema must not change between turns");
}
