//! End-to-end tests for the AGENT_LOOP node handler only.
//! Uses the wf-llm mock provider; no LLM handler is involved.

use std::sync::Arc;

use wf_execution_shared::context::NodeExecutionContext;
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::registry::ToolRegistry;
use wf_types::message::{LlmFunctionCall, LlmToolCall};
use wf_types::node::StaticNodeType;

use wf_workflow::handler::NodeHandler;

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

fn registered_echo_tool() -> Arc<ToolRegistry> {
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

#[tokio::test]
async fn agent_loop_runs_mock_driven_iterations() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"agent ping"}"#,
    )]));
    mock.script(LlmResponseSpec::text("agent final answer"));

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = wf_workflow::AgentLoopHandler::new(gateway);

    let vars = Arc::new(dashmap::DashMap::new());
    let mut ctx = NodeExecutionContext::new(
        wf_types::Id::new(),
        "agent1".to_string(),
        StaticNodeType::AgentLoop,
        serde_json::json!("do it"),
        vars,
    )
    .with_node_config(serde_json::json!({
        "inline_definition": {
            "id": "agent-1",
            "name": "mock agent",
            "created_at": 0,
            "updated_at": 0,
            "config": {
                "profile_id": "mock",
                "max_iterations": 5,
                "available_tools": {"available": ["echo"]}
            }
        }
    }));
    ctx.tool_registry = Some(registered_echo_tool());

    let result = handler.execute(&mut ctx).await.unwrap();
    assert_eq!(result.output, serde_json::json!("agent final answer"));
    assert_eq!(
        result.metadata.get("message_count").unwrap(),
        &serde_json::json!(4)
    );
    assert_eq!(mock.recorded_count(), 2);
}

#[tokio::test]
async fn agent_loop_returns_text_without_tool_calls() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("direct answer"));

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = wf_workflow::AgentLoopHandler::new(gateway);

    let vars = Arc::new(dashmap::DashMap::new());
    let mut ctx = NodeExecutionContext::new(
        wf_types::Id::new(),
        "agent2".to_string(),
        StaticNodeType::AgentLoop,
        serde_json::json!("hello"),
        vars,
    )
    .with_node_config(serde_json::json!({
        "inline_definition": {
            "id": "agent-2",
            "name": "mock agent",
            "created_at": 0,
            "updated_at": 0,
            "config": {
                "profile_id": "mock",
                "max_iterations": 3,
                "available_tools": {"available": []}
            }
        }
    }));
    ctx.tool_registry = Some(Arc::new(ToolRegistry::new()));

    let result = handler.execute(&mut ctx).await.unwrap();
    assert_eq!(result.output, serde_json::json!("direct answer"));
    assert_eq!(mock.recorded_count(), 1);
}
