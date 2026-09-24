//! End-to-end tests for LLM nodes driven by the scriptable mock provider
//! (wf-llm `mock` feature). Covers C4 (tool_calls multi-round loop, stream,
//! outputContext). Node-level retry / fallback no longer exists: LLM errors
//! propagate as node failures (real providers still retry transient faults
//! inside the wf-llm client).

use std::collections::HashMap;
use std::sync::Arc;

use wf_execution_shared::context::NodeExecutionContext;
use wf_llm::{LlmError, LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::registry::ToolRegistry;
use wf_types::message::{LlmFunctionCall, LlmToolCall, Message, MessageContentValue, MessageRole};
use wf_types::node::StaticNodeType;
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::handler::NodeHandler;
use wf_workflow::{get_context, message_context, LlmHandler, WorkflowExecutor};

fn llm_ctx(node_id: &str, config: serde_json::Value) -> NodeExecutionContext {
    let vars = Arc::new(dashmap::DashMap::new());
    NodeExecutionContext::new(
        wf_types::Id::new(),
        node_id.to_string(),
        StaticNodeType::Llm,
        serde_json::json!("hello"),
        vars,
    )
    .with_node_config(config)
}

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
async fn tool_calls_multi_round_loop_feeds_results_back() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"ping"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done after tool"));

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "tools": ["echo"],
        }),
    );
    ctx.tool_registry = Some(registered_echo_tool());

    let result = handler.execute(&mut ctx).await.unwrap();
    assert_eq!(result.output, serde_json::json!("done after tool"));
    let metadata = result.metadata;
    assert_eq!(
        metadata.get("tool_calls").unwrap(),
        &serde_json::json!([{"id": "call_1", "name": "echo", "success": true}])
    );

    // Two LLM calls: the second must carry the tool result message back.
    let requests = mock.recorded_requests();
    assert_eq!(requests.len(), 2);
    let second = &requests[1];
    let tool_msgs: Vec<&Message> = second
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .collect();
    assert_eq!(tool_msgs.len(), 1);
    assert_eq!(tool_msgs[0].tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(
        tool_msgs[0].content,
        MessageContentValue::Text(serde_json::json!({"echoed":"ping"}).to_string())
    );
    // The assistant message with the tool call must be present too.
    assert!(second
        .messages
        .iter()
        .any(|m| m.role == MessageRole::Assistant
            && m.tool_calls.as_ref().is_some_and(|c| !c.is_empty())));
}

#[tokio::test]
async fn stream_emits_chunks_and_aggregates_output() {
    let mock = Arc::new(MockLlmClient::new());
    let assistant = Message {
        id: wf_types::Id::new(),
        role: MessageRole::Assistant,
        content: MessageContentValue::Text("streamed answer".to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    };
    mock.script_stream(vec![
        wf_types::llm::MessageStreamEvent::Stream(wf_types::llm::MessageStreamChunk {
            content: "streamed ".to_string(),
        }),
        wf_types::llm::MessageStreamEvent::Stream(wf_types::llm::MessageStreamChunk {
            content: "answer".to_string(),
        }),
        wf_types::llm::MessageStreamEvent::FinalMessage(wf_types::llm::MessageStreamFinal {
            message: assistant,
            usage: Some(wf_types::llm::TokenUsageStats {
                prompt_tokens: 5,
                completion_tokens: 2,
                total_tokens: 7,
                reasoning_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                prompt_tokens_cost: None,
                completion_tokens_cost: None,
                total_cost: None,
            }),
            stream_stats: None,
        }),
        wf_types::llm::MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}),
    ]);

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);
    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "stream": true,
        }),
    );
    ctx.event_bus = Some(bus);

    let result = handler.execute(&mut ctx).await.unwrap();
    assert_eq!(result.output, serde_json::json!("streamed answer"));
    assert_eq!(
        result.metadata.get("stream").unwrap(),
        &serde_json::json!(true)
    );

    // Collect LlmStreamChunk events published to the bus.
    let mut deltas = Vec::new();
    while let Ok(event) = sub.try_recv() {
        if event.r#type == wf_types::events::EventType::LlmStreamChunk {
            let delta = event
                .metadata
                .as_ref()
                .and_then(|m| m.get("delta"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            deltas.push(delta);
        }
    }
    assert_eq!(deltas, vec!["streamed ".to_string(), "answer".to_string()]);
}

#[tokio::test]
async fn stream_error_publishes_bus_event_and_fails() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script_stream(vec![
        wf_types::llm::MessageStreamEvent::Stream(wf_types::llm::MessageStreamChunk {
            content: "partial".to_string(),
        }),
        wf_types::llm::MessageStreamEvent::Error(wf_types::llm::MessageStreamError {
            error: "HTTP 502 mid-stream".to_string(),
        }),
    ]);

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);
    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "stream": true,
        }),
    );
    ctx.event_bus = Some(bus);

    let err = match handler.execute(&mut ctx).await {
        Err(e) => e,
        Ok(_) => panic!("stream error must fail the node"),
    };
    assert!(err.to_string().contains("HTTP 502"));

    let event = loop {
        match sub.try_recv() {
            Ok(event) if event.r#type == wf_types::events::EventType::LlmStreamError => {
                break event;
            }
            Ok(_) => {}
            Err(_) => panic!("LlmStreamError must be published"),
        }
    };
    let meta = event.metadata.unwrap();
    assert_eq!(meta["error"], serde_json::json!("HTTP 502 mid-stream"));
    assert_eq!(meta["profile_id"], serde_json::json!("mock"));
    assert_eq!(
        event.execution_id.as_deref(),
        Some(ctx.execution_id.as_str())
    );
    assert!(
        event.agent_loop_id.is_none(),
        "a plain workflow LLM node is not an agent loop"
    );
}

#[tokio::test]
async fn stream_abort_publishes_bus_event_and_fails() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script_stream(vec![wf_types::llm::MessageStreamEvent::Abort(
        wf_types::llm::MessageStreamAbort {
            reason: "dead loop detected".to_string(),
        },
    )]);

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);
    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "stream": true,
        }),
    );
    ctx.event_bus = Some(bus);

    // Abort must surface as an execution error (previously swallowed).
    let err = match handler.execute(&mut ctx).await {
        Err(e) => e,
        Ok(_) => panic!("stream abort must fail the node"),
    };
    assert!(err.to_string().contains("dead loop"));

    let event = loop {
        match sub.try_recv() {
            Ok(event) if event.r#type == wf_types::events::EventType::LlmStreamAborted => {
                break event;
            }
            Ok(_) => {}
            Err(_) => panic!("LlmStreamAborted must be published"),
        }
    };
    let meta = event.metadata.unwrap();
    assert_eq!(meta["reason"], serde_json::json!("dead loop detected"));
    assert_eq!(meta["profile_id"], serde_json::json!("mock"));
}

#[tokio::test]
async fn output_context_contains_assistant_reply() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("context reply"));
    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "output_context": "out",
        }),
    );
    let result = handler.execute(&mut ctx).await.unwrap();
    assert_eq!(result.output, serde_json::json!("context reply"));

    let out = get_context(&ctx.variables, "out");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, MessageRole::Assistant);
    assert_eq!(
        out[0].content,
        MessageContentValue::Text("context reply".to_string())
    );
}

#[tokio::test]
async fn system_prompt_and_context_are_forwarded_to_mock() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("ok"));
    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let vars = Arc::new(dashmap::DashMap::new());
    message_context::append_context(
        &vars,
        "chat",
        vec![Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text("prior turn".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }],
    );
    let mut ctx = NodeExecutionContext::new(
        wf_types::Id::new(),
        "llm1".to_string(),
        StaticNodeType::Llm,
        serde_json::json!("hello"),
        vars,
    )
    .with_node_config(serde_json::json!({
        "profile_id": "mock",
        "system_prompt": "be terse",
        "context_id": "chat",
    }));
    handler.execute(&mut ctx).await.unwrap();

    let request = mock.last_request().unwrap();
    // Node input is only appended when no other messages exist, so the
    // request carries system prompt + context (no input fallback).
    let roles: Vec<MessageRole> = request.messages.iter().map(|m| m.role.clone()).collect();
    assert_eq!(roles, vec![MessageRole::System, MessageRole::User]);
    assert_eq!(
        request.messages[1].content,
        MessageContentValue::Text("prior turn".to_string())
    );
}

fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: node_type.to_string(),
        inner,
    }
}

fn graph(nodes: Vec<WorkflowNode>) -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        edges: nodes
            .windows(2)
            .map(|w| WorkflowEdge {
                id: format!("{}-{}", w[0].id, w[1].id),
                source_node_id: w[0].id.clone(),
                target_node_id: w[1].id.clone(),
                r#type: EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                error_route: None,
            })
            .collect(),
        nodes,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    }
}

fn options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(false),
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

fn llm_handlers(mock: Arc<MockLlmClient>) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let mut map: HashMap<StaticNodeType, Box<dyn NodeHandler>> = HashMap::new();
    map.insert(
        StaticNodeType::Start,
        Box::new(wf_workflow::handler::start_end::StartHandler),
    );
    map.insert(
        StaticNodeType::End,
        Box::new(wf_workflow::handler::start_end::EndHandler),
    );
    map.insert(StaticNodeType::Llm, Box::new(LlmHandler::new(gateway)));
    Arc::new(map)
}

async fn run_workflow(
    graph: WorkflowGraphStructure,
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
) -> wf_workflow::WorkflowResult<wf_tools::callback::WorkflowOutput> {
    WorkflowExecutor::new()
        .execute_workflow(wf_workflow::WorkflowRunRequest {
            workflow_id: wf_types::Id::new(),
            graph,
            options: options(),
            tool_registry: Arc::new(ToolRegistry::new()),
            handlers: Some(handlers),
            hooks: Vec::new(),
            resource_registries: None,
        })
        .await
}

#[tokio::test]
async fn llm_errors_propagate_without_node_retry() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script_error(LlmError::ProviderError("HTTP 500 boom".to_string()));
    let handlers = llm_handlers(mock.clone());

    let g = graph(vec![
        node("start", "START", serde_json::json!({})),
        node(
            "llm1",
            "LLM",
            serde_json::json!({
                "profile_id": "mock",
            }),
        ),
        node("end", "END", serde_json::json!({})),
    ]);
    let err = run_workflow(g, handlers)
        .await
        .expect_err("LLM error must fail the node");
    assert!(
        err.to_string().contains("HTTP 500 boom"),
        "unexpected error: {err}"
    );
}

/// Exhausting `max_interactions` while the model keeps emitting tool
/// calls returns the collected tool record instead of failing: the
/// structured output carries every executed call, so nothing is
/// truncated silently.
#[tokio::test]
async fn rounds_exhaustion_returns_tool_record_instead_of_error() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "c1",
        "echo",
        r#"{"text":"a"}"#,
    )]));
    // Every further call also returns tool calls: the loop can never break
    // on text.
    mock.default(LlmResponseSpec::tool_calls(vec![tool_call(
        "c2",
        "echo",
        r#"{"text":"b"}"#,
    )]));

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "tools": ["echo"],
            "max_interactions": 2,
        }),
    );
    ctx.tool_registry = Some(registered_echo_tool());

    let result = handler.execute(&mut ctx).await.unwrap();
    // Two rounds ran before the budget was spent.
    assert_eq!(mock.recorded_requests().len(), 2);
    let calls = result
        .output
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .expect("exhausted node must return a structured tool record");
    assert_eq!(calls.len(), 2);
    assert_eq!(
        result.metadata.get("rounds_exhausted"),
        Some(&serde_json::json!(true))
    );
}

/// A single response carrying more tool calls than
/// `max_tool_calls_per_request` executes the first calls and records the
/// rest as skipped failures; the error results reach the next generation
/// through the context.
#[tokio::test]
async fn per_response_cap_skips_excess_with_error_context() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![
        tool_call("c1", "echo", r#"{"text":"a"}"#),
        tool_call("c2", "echo", r#"{"text":"b"}"#),
        tool_call("c3", "echo", r#"{"text":"c"}"#),
    ]));
    mock.script(LlmResponseSpec::text("done"));

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "tools": ["echo"],
            "max_tool_calls_per_request": 2,
        }),
    );
    ctx.tool_registry = Some(registered_echo_tool());

    let result = handler.execute(&mut ctx).await.unwrap();
    assert_eq!(result.output, serde_json::json!("done"));
    let calls = result
        .metadata
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .expect("executed calls must be recorded in metadata");
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[2].get("success"), Some(&serde_json::json!(false)));
    assert_eq!(calls[2].get("skipped"), Some(&serde_json::json!(true)));

    // The skipped call surfaces to the next generation as an error tool
    // result instead of being silently dropped.
    let requests = mock.recorded_requests();
    assert_eq!(requests.len(), 2);
    let skipped_msg = requests[1]
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool && m.tool_call_id.as_deref() == Some("c3"))
        .expect("skipped call must reach the next generation as a tool result");
    match &skipped_msg.content {
        MessageContentValue::Text(text) => assert!(
            text.contains("max_tool_calls_per_request"),
            "skip reason must name the cap, got: {text}"
        ),
        other => panic!("skip result must be text, got: {other:?}"),
    }
}

/// `max_interactions: 1` is the audited single round: one generation, one
/// bounded tool round, no follow-up model call.
#[tokio::test]
async fn single_interaction_returns_tool_results_without_followup() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "c1",
        "echo",
        r#"{"text":"a"}"#,
    )]));

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "tools": ["echo"],
            "max_interactions": 1,
        }),
    );
    ctx.tool_registry = Some(registered_echo_tool());

    let result = handler.execute(&mut ctx).await.unwrap();
    assert_eq!(mock.recorded_requests().len(), 1);
    let calls = result
        .output
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .expect("single round must return a structured tool record");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].get("name"), Some(&serde_json::json!("echo")));
    assert_eq!(calls[0].get("success"), Some(&serde_json::json!(true)));
}

/// Explicit zero budgets are rejected instead of running nothing.
#[tokio::test]
async fn zero_budgets_are_rejected() {
    let gateway = Arc::new(LlmGateway::new());
    let handler = LlmHandler::new(gateway);

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({"profile_id": "mock", "max_interactions": 0}),
    );
    ctx.tool_registry = Some(registered_echo_tool());
    let err = match handler.execute(&mut ctx).await {
        Err(e) => e.to_string(),
        Ok(_) => panic!("zero max_interactions must be rejected"),
    };
    assert!(err.contains("max_interactions"));

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "tools": ["echo"],
            "max_tool_calls_per_request": 0,
        }),
    );
    ctx.tool_registry = Some(registered_echo_tool());
    let err = match handler.execute(&mut ctx).await {
        Err(e) => e.to_string(),
        Ok(_) => panic!("zero max_tool_calls_per_request must be rejected"),
    };
    assert!(err.contains("max_tool_calls_per_request"));
}

/// A configured `dead_loop_detection` block is forwarded on every
/// request the gateway receives.
#[tokio::test]
async fn dead_loop_detection_config_is_forwarded() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("done"));

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "dead_loop_detection": {"enabled": true, "checkpoints": [10, 20]},
        }),
    );
    ctx.tool_registry = Some(registered_echo_tool());

    let result = handler.execute(&mut ctx).await.unwrap();
    assert_eq!(result.output, serde_json::json!("done"));

    let requests = mock.recorded_requests();
    assert_eq!(requests.len(), 1);
    let config = requests[0]
        .dead_loop_detection
        .as_ref()
        .expect("dead_loop_detection must be forwarded");
    assert_eq!(config.enabled, Some(true));
    assert_eq!(config.checkpoints, Some(vec![10, 20]));
}

/// An invalid `dead_loop_detection` config degrades to `None` with a
/// warning; the node still executes.
#[tokio::test]
async fn invalid_dead_loop_detection_degrades_to_none() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("done"));

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let mut ctx = llm_ctx(
        "llm1",
        serde_json::json!({
            "profile_id": "mock",
            "dead_loop_detection": {"enabled": "not-a-bool"},
        }),
    );
    ctx.tool_registry = Some(registered_echo_tool());

    let result = handler.execute(&mut ctx).await.unwrap();
    assert_eq!(result.output, serde_json::json!("done"));
    assert!(mock
        .recorded_requests()
        .first()
        .and_then(|r| r.dead_loop_detection.as_ref())
        .is_none());
}
