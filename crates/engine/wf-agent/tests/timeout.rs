//! Timeout integration tests (`timeout::AgentTimeoutManager` + wall-clock
//! and pause budgets through the loop): slow runs fail with a timeout and
//! pause budgets stop a paused entity.

use std::sync::Arc;
use std::time::Duration;

use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_agent::timeout::AgentTimeoutManager;
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_tools::registry::ToolRegistry;

fn gateway_with(mock: Arc<MockLlmClient>) -> Arc<LlmGateway> {
    let gateway = LlmGateway::new();
    gateway.register_mock("mock", mock);
    Arc::new(gateway)
}

fn registry() -> Arc<ToolRegistry> {
    Arc::new(ToolRegistry::new())
}

fn config() -> AgentLoopConfig {
    AgentLoopConfig {
        agent_id: "timeout-agent".to_string(),
        model: "mock".to_string(),
        max_iterations: Some(5),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: Vec::new(),
        initial_tool_names: Vec::new(),
        discoverable_tool_names: Vec::new(),
        enable_general_tool: None,
        activated_tool_names: Vec::new(),
        hidden_tool_names: Vec::new(),
        tool_call_protocol: None,
        token_limit: None,
        token_warning_threshold: None,
        enable_token_tracking: Some(false),
        general_description: None,
        discoverable_metadata_block: None,
        history_normalization: false,
        checkpoint_message_interval: None,
    }
}

fn input() -> AgentLoopInput {
    AgentLoopInput {
        message: "run".to_string(),
        context: std::collections::HashMap::new(),
        conversation: Vec::new(),
    }
}

#[tokio::test]
async fn timeout_manager_fires_once_and_cancel_suppresses() {
    use std::sync::atomic::{AtomicU32, Ordering};

    let manager = AgentTimeoutManager::new();
    let fired = Arc::new(AtomicU32::new(0));
    let clone = fired.clone();
    manager.register("t-fire", Duration::from_millis(30), move || {
        clone.fetch_add(1, Ordering::SeqCst);
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(fired.load(Ordering::SeqCst), 1);

    let suppressed = Arc::new(AtomicU32::new(0));
    let clone = suppressed.clone();
    let handle = manager.register("t-cancel", Duration::from_millis(30), move || {
        clone.fetch_add(1, Ordering::SeqCst);
    });
    handle.cancel();
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(suppressed.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn wall_clock_timeout_fails_a_slow_loop() {
    let mock = Arc::new(MockLlmClient::new());
    mock.default(
        LlmResponseSpec::text("slow final")
            .with_usage(10, 5)
            .with_delay(500),
    );
    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry());
    let mut cfg = config();
    cfg.max_execution_time = Some(40);
    let err = coordinator.execute(cfg, input()).await.unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("timeout")
            || err.to_string().to_lowercase().contains("stopped")
            || err.to_string().to_lowercase().contains("exceed"),
        "slow run must fail with a timeout/stop error: {err}"
    );
}

#[tokio::test]
async fn fast_loop_completes_under_generous_budget() {
    let mock = Arc::new(MockLlmClient::new());
    mock.default(LlmResponseSpec::text("fast final").with_usage(10, 5));
    let coordinator = AgentLoopCoordinator::new(gateway_with(mock), registry());
    let mut cfg = config();
    cfg.max_execution_time = Some(5000);
    let output = coordinator.execute(cfg, input()).await.unwrap();
    assert_eq!(output.result, serde_json::json!("fast final"));
}

#[tokio::test]
async fn pause_budget_stops_a_paused_entity() {
    use wf_core::interruption::InterruptionSignal;
    use wf_execution_shared::types::execution_entity::ExecutionEntity;
    use wf_types::Id;

    let entity = wf_agent::entity::AgentLoopEntity::new(Id::from("pause-budget-1".to_string()))
        .with_max_pause_duration(30);
    entity.state.write().await.start().unwrap();
    entity.pause().await.expect("pause must succeed");
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(entity.interruption().is_interrupted());
    assert_eq!(
        entity.interruption().check(),
        Some(InterruptionSignal::Stop)
    );

    entity.resume().await.expect("resume must succeed");
    assert!(!entity.interruption().is_interrupted());
}
