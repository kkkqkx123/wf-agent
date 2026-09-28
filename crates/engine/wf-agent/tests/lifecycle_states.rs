//! Agent loop terminal states and pause/resume through the public
//! `AgentLoopCoordinator` API: fail/cancel/timeout settlement and the
//! pause-resume round trip plus the iteration cap.
//!
//! Covers `coordinator::lifecycle` settlement (fail/cancel/timeout) and the
//! suspension gate end to end. Entity state is read back through a shared
//! `AgentLoopRegistry` with a fixed loop id; events are asserted on the
//! `EventBus` with the drain pattern used by the token tests.

use std::sync::Arc;
use std::time::Duration;

use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_agent::registry::AgentLoopRegistry;
use wf_execution_shared::types::execution_entity::{ExecutionEntity, ExecutionStatus};
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, LoopFinishReason};
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

fn bus_event_types(sub: &mut wf_core::event::Subscription) -> Vec<wf_types::events::EventType> {
    let mut types = Vec::new();
    while let Ok(event) = sub.try_recv() {
        types.push(event.r#type);
    }
    types
}

#[tokio::test]
async fn fail_settlement_marks_failed_and_publishes_agent_failed() {
    use wf_llm::LlmError;

    let mock = Arc::new(MockLlmClient::new());
    mock.script_error(LlmError::AuthError("invalid key".to_string()));

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();
    let registry = Arc::new(AgentLoopRegistry::new());
    let loop_id = wf_types::Id::from("lifecycle-fail-1".to_string());

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry_with_echo())
        .with_event_bus(bus)
        .with_entity_registry(registry.clone())
        .with_agent_loop_id(loop_id.clone());
    let err = coordinator
        .execute(config(3), input("do it"))
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("invalid key"),
        "root cause must surface: {err}"
    );

    let entity = registry.get(&loop_id).expect("failed run stays registered");
    assert_eq!(entity.state.read().await.status(), ExecutionStatus::Failed);
    assert!(
        entity
            .state
            .read()
            .await
            .error()
            .unwrap_or_default()
            .contains("invalid key"),
        "entity error carries the root cause"
    );

    let types = bus_event_types(&mut sub);
    assert!(
        types.contains(&wf_types::events::EventType::AgentFailed),
        "AgentFailed must be published: {types:?}"
    );
    assert!(
        !types.contains(&wf_types::events::EventType::AgentCompleted),
        "no completion leak on failure: {types:?}"
    );
}

#[tokio::test]
async fn stop_during_run_settles_cancelled_not_failed() {
    let mock = Arc::new(MockLlmClient::new());
    mock.default(LlmResponseSpec::text("late final").with_delay(800));

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();
    let registry = Arc::new(AgentLoopRegistry::new());
    let loop_id = wf_types::Id::from("lifecycle-cancel-1".to_string());

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry_with_echo())
        .with_event_bus(bus)
        .with_entity_registry(registry.clone())
        .with_agent_loop_id(loop_id.clone());
    let handle = tokio::spawn(async move { coordinator.execute(config(5), input("run")).await });

    // Wait for registration, then force-stop the live entity mid-LLM-call.
    let entity = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(entity) = registry.get(&loop_id) {
                return entity;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("entity registers promptly");
    tokio::time::sleep(Duration::from_millis(80)).await;
    entity.stop().await.expect("stop must succeed");

    let outcome = tokio::time::timeout(Duration::from_secs(10), handle)
        .await
        .expect("run must settle")
        .expect("task joins");
    let err = outcome.unwrap_err();
    assert!(
        matches!(err, wf_agent::error::AgentError::Cancelled(_)),
        "explicit stop settles as Cancelled, not Failed: {err}"
    );
    assert_eq!(
        entity.state.read().await.status(),
        ExecutionStatus::Cancelled
    );

    let types = bus_event_types(&mut sub);
    assert!(
        types.contains(&wf_types::events::EventType::AgentCancelled),
        "AgentCancelled must be published: {types:?}"
    );
    assert!(
        !types.contains(&wf_types::events::EventType::AgentFailed),
        "cancel must not report as failure: {types:?}"
    );
    assert!(
        !types.contains(&wf_types::events::EventType::AgentCompleted),
        "no completion leak on cancel: {types:?}"
    );
}

#[tokio::test]
async fn wall_clock_timeout_settles_timeout_state() {
    let mock = Arc::new(MockLlmClient::new());
    mock.default(LlmResponseSpec::text("slow final").with_delay(800));

    let registry = Arc::new(AgentLoopRegistry::new());
    let loop_id = wf_types::Id::from("lifecycle-timeout-1".to_string());
    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry_with_echo())
        .with_entity_registry(registry.clone())
        .with_agent_loop_id(loop_id.clone());
    let mut cfg = config(5);
    cfg.max_execution_time = Some(40);
    let err = coordinator.execute(cfg, input("run")).await.unwrap_err();
    assert!(
        matches!(err, wf_agent::error::AgentError::ExecutionTimeout(_)),
        "slow run settles as ExecutionTimeout: {err}"
    );
    let entity = registry
        .get(&loop_id)
        .expect("timed-out run stays registered");
    assert_eq!(entity.state.read().await.status(), ExecutionStatus::Timeout);
}

#[tokio::test]
async fn pause_then_resume_converges_to_completed() {
    use wf_core::internal_signal::{InternalSignal, InternalSignalBus};

    let loop_id = "lifecycle-pause-1".to_string();
    let signal_bus = Arc::new(InternalSignalBus::new());
    let publish_bus = signal_bus.clone();

    // First tool call parks the loop: the pause signal lands at the next
    // iteration boundary through the typed signal bus.
    let parked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let parked_flag = parked.clone();
    let pause_target: wf_types::Id = loop_id.clone();
    let pause_source: wf_types::Id = "pause-test".to_string();
    let registry = Arc::new(ToolRegistry::new());
    registry.register_stateless_handler(
        "echo",
        Arc::new(move |params, _ctx| {
            if !parked_flag.swap(true, std::sync::atomic::Ordering::SeqCst) {
                publish_bus.publish(InternalSignal::PauseWorkflow {
                    source: pause_source.clone(),
                    target_execution_id: pause_target.clone(),
                    reason: Some("pause for resume test".to_string()),
                });
            }
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

    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"ping"}"#,
    )]));
    mock.script(LlmResponseSpec::text("recovered"));

    let entity_registry = Arc::new(AgentLoopRegistry::new());
    let agent_loop_id = wf_types::Id::from(loop_id.clone());
    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry)
        .with_signal_bus(signal_bus.clone())
        .with_entity_registry(entity_registry.clone())
        .with_agent_loop_id(agent_loop_id.clone());
    let run = tokio::spawn(async move { coordinator.execute(config(5), input("run")).await });

    // Wait until the loop parks at the iteration boundary, then resume it.
    // The signal path suspends the interruption gate; without a checkpoint
    // integration the state machine itself stays Running while parked.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(entity) = entity_registry.get(&agent_loop_id) {
                if entity.interruption().check()
                    == Some(wf_core::interruption::InterruptionSignal::Pause)
                {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("loop must pause at the iteration boundary");
    // Resume through the live entity handle: the parked loop waits on the
    // interruption gate, which only an explicit resume flips.
    let parked_entity = entity_registry
        .get(&agent_loop_id)
        .expect("parked entity stays registered");
    parked_entity.resume().await.expect("resume must succeed");

    let output = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("resumed run must settle")
        .expect("task joins")
        .expect("resumed run converges");
    assert_eq!(output.result, serde_json::json!("recovered"));
    assert_eq!(output.iterations, 2);
    let entity = entity_registry
        .get(&agent_loop_id)
        .expect("resumed run stays registered");
    assert_eq!(
        entity.state.read().await.status(),
        ExecutionStatus::Completed
    );
}

#[tokio::test]
async fn iteration_cap_terminates_with_max_iterations_reason() {
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
    assert_eq!(output.finish_reason, LoopFinishReason::MaxIterationsReached);
    assert_eq!(
        output.result,
        serde_json::json!("Max iterations reached"),
        "uncapped loops terminate with the cap sentinel, not a model answer"
    );
}
