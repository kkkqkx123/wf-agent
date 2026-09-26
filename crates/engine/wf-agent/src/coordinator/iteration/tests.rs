use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc;
use wf_llm::mock::MockLlmClient;
use wf_llm::LlmGateway;
use wf_types::llm::MessageStreamEvent;
use wf_types::message::{LlmFunctionCall, LlmToolCall, Message, MessageContentValue};
use wf_types::Id;

use super::{AgentIterationCoordinator, IterationResult};
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;
use crate::stream::{AgentEventSink, AgentStreamEvent};

fn tool_call_message(tool_call_id: &str) -> Message {
    Message {
        id: Id::from(wf_common::generate_id()),
        role: wf_types::message::MessageRole::Assistant,
        content: MessageContentValue::Text("using tool".to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: Some(vec![LlmToolCall {
            id: tool_call_id.to_string(),
            r#type: "function".to_string(),
            function: LlmFunctionCall {
                name: "mock_write".to_string(),
                arguments: "{}".to_string(),
            },
        }]),
        thinking: None,
        metadata: None,
    }
}

fn text_message(content: &str) -> Message {
    Message {
        id: Id::from(wf_common::generate_id()),
        role: wf_types::message::MessageRole::Assistant,
        content: MessageContentValue::Text(content.to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

fn mock_tool_registry(
    executed: &Arc<std::sync::atomic::AtomicU32>,
) -> Arc<wf_tools::registry::ToolRegistry> {
    use std::sync::atomic::Ordering;
    let registry = Arc::new(wf_tools::registry::ToolRegistry::new());
    let handler: wf_tools::executor::stateless::StatelessHandler = {
        let executed = executed.clone();
        Arc::new(
            move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                executed.fetch_add(1, Ordering::SeqCst);
                Ok(Value::from("stream-tool-ok"))
            },
        )
    };
    registry.register_tool(wf_types::tool::Tool {
        id: "tool-1".to_string(),
        name: "mock_write".to_string(),
        description: "mock".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: None,
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: Some(5000),
    });
    registry.register_stateless_handler("tool-1", handler);
    registry
}

fn stream_gateway(mock: Arc<MockLlmClient>) -> Arc<LlmGateway> {
    let gateway = LlmGateway::new();
    gateway.register_mock("mock", mock);
    Arc::new(gateway)
}

async fn run_streaming(
    gateway: Arc<LlmGateway>,
    registry: Arc<wf_tools::registry::ToolRegistry>,
    entity: &AgentLoopEntity,
) -> (
    AgentResult<IterationResult>,
    mpsc::Receiver<AgentStreamEvent>,
) {
    let (tx, rx) = mpsc::channel(32);
    let coordinator = AgentIterationCoordinator::new(gateway, registry, None)
        .with_streaming(AgentEventSink::new(tx, None));
    (coordinator.execute_iteration(entity).await, rx)
}

#[tokio::test]
async fn test_stream_events_text_only() {
    let executed = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let mock = Arc::new(MockLlmClient::new());
    mock.script_stream(vec![
        MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
            text: "hello ".to_string(),
            snapshot: String::new(),
        }),
        MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
            text: "world".to_string(),
            snapshot: String::new(),
        }),
        MessageStreamEvent::FinalMessage(wf_types::llm::MessageStreamFinal {
            message: text_message("hello world"),
            usage: None,
            stream_stats: None,
        }),
        MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}),
    ]);
    let entity = AgentLoopEntity::new(Id::from("agent-stream-1".to_string()))
        .with_model("mock".to_string());
    entity.state.write().await.start().unwrap();

    let (result, mut rx) = run_streaming(stream_gateway(mock), registry, &entity).await;
    let result = result.expect("stream iteration must succeed");

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    assert_eq!(events.len(), 4); // IterationStart + 2 deltas + IterationEnd
                                 // The iteration boundary carries the conversation turn anchor
                                 // (message_count / array_version) for trigger conditions and
                                 // nested-agent input slicing.
    match &events[0] {
        AgentStreamEvent::IterationStart {
            iteration,
            message_count: 0,
            array_version: 0,
            ..
        } => assert_eq!(*iteration, 1),
        other => panic!("expected IterationStart, got {:?}", other),
    }
    assert!(
        matches!(&events[1], AgentStreamEvent::LlmDelta { content } if content == "hello ")
    );
    assert!(matches!(&events[2], AgentStreamEvent::LlmDelta { content } if content == "world"));
    // The iteration appended one assistant message: the end boundary
    // anchor reflects the conversation after the iteration.
    match &events[3] {
        AgentStreamEvent::IterationEnd {
            iteration: 1,
            message_count: 1,
            array_version,
            ..
        } => assert!(*array_version > 0),
        other => panic!("expected IterationEnd, got {:?}", other),
    }

    // No tool calls -> complete immediately with content.
    assert!(!result.should_continue);
    assert_eq!(result.content, Value::String("hello world".to_string()));
    assert_eq!(executed.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn test_stream_events_with_tool_call() {
    let executed = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let mock = Arc::new(MockLlmClient::new());
    mock.script_stream(vec![
        MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
            text: "using tool".to_string(),
            snapshot: String::new(),
        }),
        MessageStreamEvent::FinalMessage(wf_types::llm::MessageStreamFinal {
            message: tool_call_message("tc-9"),
            usage: None,
            stream_stats: None,
        }),
        MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}),
    ]);
    let entity = AgentLoopEntity::new(Id::from("agent-stream-2".to_string()))
        .with_model("mock".to_string());
    entity.state.write().await.start().unwrap();

    let (result, mut rx) = run_streaming(stream_gateway(mock), registry, &entity).await;
    let result = result.expect("stream iteration with tool must succeed");

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    let tool_start = events
        .iter()
        .find(|e| matches!(e, AgentStreamEvent::ToolStart { .. }));
    let tool_end = events
        .iter()
        .find(|e| matches!(e, AgentStreamEvent::ToolEnd { .. }));
    assert!(tool_start.is_some(), "ToolStart missing: {:?}", events);
    assert!(tool_end.is_some(), "ToolEnd missing: {:?}", events);
    if let Some(AgentStreamEvent::ToolEnd {
        success, result: r, ..
    }) = tool_end
    {
        assert!(*success);
        assert!(r.contains("stream-tool-ok"), "unexpected result: {}", r);
    }
    assert_eq!(executed.load(std::sync::atomic::Ordering::SeqCst), 1);
    // Tool call was mock_write (not attempt_completion) -> keep looping.
    assert!(result.should_continue);
    assert_eq!(result.tool_call_count, 1);
    // Tool message added to conversation (assistant + tool).
    let messages = entity.conversation().read().await.messages().to_vec();
    assert_eq!(messages.len(), 2);
}

#[tokio::test]
async fn test_stream_error_propagates() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script_error(wf_llm::error::LlmError::StreamError(
        "upstream exploded".to_string(),
    ));
    let registry = Arc::new(wf_tools::registry::ToolRegistry::new());
    let entity = AgentLoopEntity::new(Id::from("agent-stream-3".to_string()))
        .with_model("mock".to_string());
    entity.state.write().await.start().unwrap();

    let (result, _rx) = run_streaming(stream_gateway(mock), registry, &entity).await;
    let err = result.expect_err("stream error must fail the iteration");
    assert!(err.to_string().contains("upstream exploded"));
}

async fn run_streaming_with_bus(
    gateway: Arc<LlmGateway>,
    registry: Arc<wf_tools::registry::ToolRegistry>,
    entity: &AgentLoopEntity,
    bus: Arc<wf_core::EventBus>,
) -> AgentResult<IterationResult> {
    let (tx, _rx) = mpsc::channel(32);
    let coordinator = AgentIterationCoordinator::new(gateway, registry, None)
        .with_streaming(AgentEventSink::new(tx, None))
        .with_event_bus(bus);
    coordinator.execute_iteration(entity).await
}

fn drain_until_type(
    sub: &mut wf_core::event::Subscription,
    event_type: wf_types::events::EventType,
) -> Option<wf_types::events::BaseEvent> {
    for _ in 0..16 {
        match sub.try_recv() {
            Ok(event) if event.r#type == event_type => return Some(event),
            Ok(_) => {}
            Err(_) => break,
        }
    }
    None
}

#[tokio::test]
async fn test_stream_error_event_published_to_bus() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script_stream(vec![MessageStreamEvent::Error(
        wf_types::llm::MessageStreamError {
            error: "HTTP 500 boom".to_string(),
        },
    )]);
    let registry = Arc::new(wf_tools::registry::ToolRegistry::new());
    let entity = AgentLoopEntity::new(Id::from("agent-stream-err".to_string()))
        .with_model("mock".to_string());
    entity.state.write().await.start().unwrap();

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let result =
        run_streaming_with_bus(stream_gateway(mock), registry, &entity, bus.clone()).await;
    assert!(result.is_err(), "stream error must fail the iteration");

    let event = drain_until_type(&mut sub, wf_types::events::EventType::LlmStreamError)
        .expect("LlmStreamError must be published");
    assert_eq!(event.execution_id.as_deref(), Some("agent-stream-err"));
    assert_eq!(event.agent_loop_id.as_deref(), Some("agent-stream-err"));
    let meta = event.metadata.unwrap();
    assert_eq!(meta["error"], serde_json::json!("HTTP 500 boom"));
    assert_eq!(meta["profile_id"], serde_json::json!("mock"));
}

#[tokio::test]
async fn test_stream_abort_event_published_to_bus() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script_stream(vec![MessageStreamEvent::Abort(
        wf_types::llm::MessageStreamAbort {
            reason: "dead loop detected".to_string(),
        },
    )]);
    let registry = Arc::new(wf_tools::registry::ToolRegistry::new());
    let entity = AgentLoopEntity::new(Id::from("agent-stream-abort".to_string()))
        .with_model("mock".to_string());
    entity.state.write().await.start().unwrap();

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let result =
        run_streaming_with_bus(stream_gateway(mock), registry, &entity, bus.clone()).await;
    assert!(result.is_err(), "stream abort must fail the iteration");

    let event = drain_until_type(&mut sub, wf_types::events::EventType::LlmStreamAborted)
        .expect("LlmStreamAborted must be published");
    let meta = event.metadata.unwrap();
    assert_eq!(meta["reason"], serde_json::json!("dead loop detected"));
    assert_eq!(meta["profile_id"], serde_json::json!("mock"));
}
