//! Stream lifecycle tests (`stream::AgentEventStream` + bus mirror):
//! dropping the consumer stops the run, and every `AgentStreamEvent`
//! variant maps onto its documented bus `EventType` with the event JSON
//! as metadata.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_agent::stream::{AgentEventSink, AgentStreamEvent};
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_tools::registry::ToolRegistry;
use wf_types::events::EventType;
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
async fn dropped_consumer_stops_further_consumption() {
    let mock = Arc::new(MockLlmClient::new());
    mock.default(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"loop"}"#,
    )]));
    // Streaming iterations consume the mock through `generate_stream`,
    // where pacing comes from the per-event stream delay (the blocking
    // `with_delay` does not apply). Slow the stream so the drop lands
    // while the first iteration is still in flight.
    mock.with_stream_delay(300);

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), registry_with_echo());
    let mut stream = coordinator.execute_stream(config(20), input("run")).await;

    // Consume exactly one event, then drop the consumer: the internal task
    // aborts instead of driving the remaining iterations.
    let first = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("first event arrives")
        .expect("stream opens with an event");
    assert!(
        !matches!(first, AgentStreamEvent::Completed { .. }),
        "dropping happens before completion: {first:?}"
    );
    drop(stream);

    let at_drop = mock.recorded_count();
    assert!(
        at_drop <= 2,
        "only the in-flight iteration may have run: {at_drop}"
    );
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(
        mock.recorded_count(),
        at_drop,
        "aborted run must not keep consuming the mock script"
    );
}

#[tokio::test]
async fn bus_mirror_maps_every_stream_variant() {
    let bus = Arc::new(wf_core::EventBus::new(128));
    let mut sub = bus.subscribe();
    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let sink = AgentEventSink::new(tx, Some(bus));

    let cases: Vec<(AgentStreamEvent, EventType)> = vec![
        (
            AgentStreamEvent::IterationStart {
                iteration: 1,
                message_count: 0,
                array_version: 0,
            },
            EventType::AgentIterationStarted,
        ),
        (
            AgentStreamEvent::LlmDelta {
                content: "hi".to_string(),
            },
            EventType::LlmStreamChunk,
        ),
        (
            AgentStreamEvent::ToolStart {
                tool_call_id: "call_1".to_string(),
                tool_name: "echo".to_string(),
            },
            EventType::AgentToolExecutionStarted,
        ),
        (
            AgentStreamEvent::ToolEnd {
                tool_call_id: "call_1".to_string(),
                tool_name: "echo".to_string(),
                success: true,
                result: "ok".to_string(),
                error: None,
            },
            EventType::AgentToolExecutionCompleted,
        ),
        (
            AgentStreamEvent::IterationEnd {
                iteration: 1,
                message_count: 2,
                array_version: 3,
            },
            EventType::AgentIterationCompleted,
        ),
        (
            AgentStreamEvent::Completed {
                result: serde_json::json!("done"),
                iterations: 1,
            },
            EventType::AgentCompleted,
        ),
        (
            AgentStreamEvent::Failed {
                error: "boom".to_string(),
                error_type: wf_types::errors::ErrorType::Internal,
                permanently_failed_tools: vec!["echo".to_string()],
            },
            EventType::AgentFailed,
        ),
        (
            AgentStreamEvent::Interrupted {
                reason: "stopped".to_string(),
            },
            EventType::AgentCancelled,
        ),
        (
            AgentStreamEvent::ReasoningDelta {
                content: "thinking".to_string(),
            },
            EventType::LlmStreamChunk,
        ),
        (
            AgentStreamEvent::Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                cost: None,
            },
            EventType::LlmStreamDone,
        ),
        (
            AgentStreamEvent::SubAgentStarted {
                id: "child-1".to_string(),
                name: "child".to_string(),
            },
            EventType::AgentStarted,
        ),
        (
            AgentStreamEvent::SubAgentEnded {
                id: "child-1".to_string(),
                name: "child".to_string(),
                success: true,
            },
            EventType::AgentCompleted,
        ),
    ];

    for (event, expected) in &cases {
        sink.emit("loop-mirror-1", event.clone())
            .await
            .expect("receiver alive during the mapping probe");
        let bus_event = sub.try_recv().expect("mirror publishes to the bus");
        assert_eq!(bus_event.r#type, *expected, "mapping for {event:?}");
        assert_eq!(
            bus_event.execution_id.as_deref(),
            Some("loop-mirror-1")
        );
        assert_eq!(
            bus_event.agent_loop_id.as_deref(),
            Some("loop-mirror-1")
        );
        let expected_metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::to_value(event)
                .expect("stream event serializes")
                .as_object()
                .expect("stream event is a JSON object")
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        assert_eq!(
            bus_event.metadata.as_ref(),
            Some(&expected_metadata),
            "metadata is the event JSON object for {event:?}"
        );
    }
}

#[tokio::test]
async fn streaming_run_mirrors_lifecycle_to_bus() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"hi"}"#,
    )]));
    mock.script(LlmResponseSpec::text("done"));

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();
    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry_with_echo())
        .with_event_bus(bus);
    let mut stream = coordinator.execute_stream(config(5), input("run")).await;
    let mut completed = None;
    while let Some(event) = stream.next().await {
        if let AgentStreamEvent::Completed { result, .. } = &event {
            completed = Some(result.clone());
        }
    }
    assert_eq!(completed, Some(serde_json::json!("done")));

    let mut types = Vec::new();
    while let Ok(event) = sub.try_recv() {
        types.push(event.r#type);
    }
    for expected in [
        EventType::AgentStarted,
        EventType::AgentIterationStarted,
        EventType::AgentToolExecutionStarted,
        EventType::AgentToolExecutionCompleted,
        EventType::AgentIterationCompleted,
        EventType::AgentCompleted,
    ] {
        assert!(
            types.contains(&expected),
            "{expected:?} mirrored to the bus: {types:?}"
        );
    }
}
