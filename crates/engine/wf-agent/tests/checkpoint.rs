//! Checkpoint integration tests (`checkpoint::{strategy,coordinator}` +
//! lifecycle resume): idempotent replay, message-interval backstop and
//! branch vs in-place resume modes.

use std::sync::Arc;

use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_llm::{LlmError, LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_storage::backend::StorageBackend;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
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

fn counting_registry(runs: Arc<std::sync::atomic::AtomicUsize>) -> Arc<ToolRegistry> {
    use std::sync::atomic::Ordering;

    let registry = Arc::new(ToolRegistry::new());
    let counter = runs.clone();
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

/// Drive an agent loop to failure: iteration 1 records an `echo` tool call,
/// the second LLM call fails with a non-retryable auth error. The coordinator
/// and mock come back so the test can script the resume on top of the
/// checkpoint history the interrupted run left behind.
async fn interrupted_run(
    loop_id: &str,
    store: Arc<StorageBackend>,
    registry: Arc<ToolRegistry>,
) -> (AgentLoopCoordinator, Arc<MockLlmClient>) {
    use wf_agent::AgentCheckpointStrategy;
    use wf_types::Id;

    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"ping"}"#,
    )]));
    mock.script_error(LlmError::AuthError("interrupted".to_string()));

    let coordinator = AgentLoopCoordinator::with_store(gateway_with(mock.clone()), registry, store)
        .with_agent_loop_id(Id::from(loop_id.to_string()))
        .with_checkpoint_strategy(AgentCheckpointStrategy::from_agent_config(
            1, true, false, false, None,
        ));

    let err = coordinator
        .execute(config(5), input("first run"))
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("interrupted"),
        "first run must be interrupted: {err}"
    );
    (coordinator, mock)
}

/// The two snapshots an interrupted run is resumed from: the error boundary
/// (taken while the loop was still running, the last live snapshot) and the
/// settle snapshot recorded after the loop settled. `resume` continues only
/// the former, so tests select by trigger tag instead of list position.
async fn resume_sources(store: &Arc<StorageBackend>, entity_id: &str) -> (String, String) {
    use wf_checkpoint::state::agent::AgentCheckpointStateManager;
    use wf_checkpoint::state::CheckpointStateManager;

    let sm = AgentCheckpointStateManager::new(store.clone());
    let rows = sm.list_by_entity(entity_id).await.expect("checkpoint list");
    let tagged = |tag: &str| {
        rows.iter()
            .find(|row| {
                row.tags
                    .as_ref()
                    .is_some_and(|tags| tags.iter().any(|t| t == tag))
            })
            .map(|row| row.id.to_string())
    };
    let live = tagged("trigger:ON_ERROR").expect("error boundary snapshot exists");
    let settle = tagged("trigger:ON_FAILURE").expect("settle snapshot exists");
    (live, settle)
}

#[tokio::test]
async fn resume_from_checkpoint_replays_idempotent_tool_calls() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let echo_runs = Arc::new(AtomicUsize::new(0));
    let registry = counting_registry(echo_runs.clone());
    let store = Arc::new(StorageBackend::new_memory());
    let (coordinator, mock) =
        interrupted_run("restore-loop", store.clone(), registry.clone()).await;
    assert_eq!(echo_runs.load(Ordering::SeqCst), 1, "echo executed once");

    let (live, _) = resume_sources(&store, "restore-loop").await;

    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"ping"}"#,
    )]));
    mock.script(LlmResponseSpec::text("recovered"));

    let output = coordinator
        .resume_from_checkpoint_in_place(&live, config(5), input("continue"))
        .await
        .unwrap();
    assert_eq!(output.result, serde_json::json!("recovered"));
    assert!(
        output.iterations >= 2,
        "resumed run continues beyond the restored iteration count"
    );
    assert_eq!(
        echo_runs.load(Ordering::SeqCst),
        1,
        "replayed tool call served from the idempotency cache, not re-executed"
    );
}

#[tokio::test]
async fn checkpoint_message_interval_produces_interval_checkpoints() {
    use wf_checkpoint::state::agent::AgentCheckpointStateManager;
    use wf_checkpoint::state::CheckpointStateManager;

    let store = Arc::new(StorageBackend::new_memory());
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("final answer"));

    let coordinator =
        AgentLoopCoordinator::with_store(gateway_with(mock), registry_with_echo(), store.clone())
            .with_agent_loop_id(wf_types::Id::from("interval-loop"));

    let mut cfg = config(5);
    cfg.checkpoint_message_interval = Some(2);
    let output = coordinator
        .execute(cfg, input("keep talking"))
        .await
        .unwrap();
    assert_eq!(output.result, serde_json::json!("final answer"));

    let sm = AgentCheckpointStateManager::new(store.clone());
    let all = sm.list_by_entity("interval-loop").await.unwrap();
    assert!(!all.is_empty(), "interval run must persist checkpoints");
    let mut saw_interval = false;
    for meta in &all {
        let cp = sm
            .load(&meta.id)
            .await
            .unwrap()
            .expect("listed checkpoint blob must load");
        let tags = cp
            .metadata
            .as_ref()
            .and_then(|m| m.get("tags"))
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        if tags.iter().any(|t| t.as_str() == Some("trigger:INTERVAL")) {
            saw_interval = true;
            break;
        }
    }
    assert!(
        saw_interval,
        "expected an Interval checkpoint among {} checkpoints",
        all.len()
    );

    let plain_store = Arc::new(StorageBackend::new_memory());
    let plain_mock = Arc::new(MockLlmClient::new());
    plain_mock.script(LlmResponseSpec::text("final answer"));
    let plain = AgentLoopCoordinator::with_store(
        gateway_with(plain_mock),
        registry_with_echo(),
        plain_store.clone(),
    );
    let plain_output = plain.execute(config(5), input("quiet run")).await.unwrap();
    let plain_sm = AgentCheckpointStateManager::new(plain_store);
    assert_eq!(
        plain_sm
            .count_by_entity(&plain_output.agent_loop_id)
            .await
            .unwrap(),
        0,
        "unconfigured runs must not persist checkpoints"
    );
}

#[tokio::test]
async fn in_place_resume_continues_under_source_execution_id() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use wf_agent::AgentCheckpointStrategy;
    use wf_checkpoint::state::agent::AgentCheckpointStateManager;
    use wf_checkpoint::state::CheckpointStateManager;
    use wf_types::Id;

    let echo_runs = Arc::new(AtomicUsize::new(0));
    let registry = counting_registry(echo_runs.clone());
    let store = Arc::new(StorageBackend::new_memory());
    let (coordinator, mock) =
        interrupted_run("inplace-loop", store.clone(), registry.clone()).await;

    let sm = AgentCheckpointStateManager::new(store.clone());
    let (live, _) = resume_sources(&store, "inplace-loop").await;
    let count_before = sm.count_by_entity("inplace-loop").await.unwrap();

    mock.script(LlmResponseSpec::tool_calls(vec![tool_call(
        "call_1",
        "echo",
        r#"{"text":"ping"}"#,
    )]));
    mock.script(LlmResponseSpec::text("recovered"));
    let output = coordinator
        .resume_from_checkpoint_in_place(&live, config(5), input("continue"))
        .await
        .unwrap();
    assert_eq!(output.agent_loop_id, "inplace-loop");
    assert_eq!(output.result, serde_json::json!("recovered"));
    assert_eq!(
        echo_runs.load(Ordering::SeqCst),
        1,
        "replayed tool call served from the idempotency cache, not re-executed"
    );
    let count_after = sm.count_by_entity("inplace-loop").await.unwrap();
    assert!(
        count_after > count_before,
        "in-place resume must append checkpoints under the source id"
    );
    let latest = sm
        .get_latest("inplace-loop")
        .await
        .unwrap()
        .expect("checkpoints remain under the source id");
    assert_eq!(latest.entity_id, "inplace-loop");

    mock.script(LlmResponseSpec::text("branched"));
    let branch_coordinator = AgentLoopCoordinator::with_store(
        gateway_with(mock.clone()),
        registry.clone(),
        store.clone(),
    )
    .with_agent_loop_id(Id::from("branch-loop"))
    .with_checkpoint_strategy(AgentCheckpointStrategy::from_agent_config(
        1, true, false, false, None,
    ));
    let branch_output = branch_coordinator
        .resume_from_checkpoint(&live, config(5), input("branch off"))
        .await
        .unwrap();
    assert_eq!(branch_output.agent_loop_id, "branch-loop");
    assert_ne!(
        branch_output.agent_loop_id, "inplace-loop",
        "branch resume must not reuse the source execution id"
    );

    let same_id_err = coordinator
        .resume_from_checkpoint(&live, config(5), input("bad branch"))
        .await
        .unwrap_err();
    assert!(
        same_id_err.to_string().contains("fresh execution id"),
        "branch resume must reject source id reuse: {same_id_err}"
    );
    let other = AgentLoopCoordinator::with_store(
        gateway_with(mock.clone()),
        registry.clone(),
        store.clone(),
    )
    .with_agent_loop_id(Id::from("other-id"))
    .with_checkpoint_strategy(AgentCheckpointStrategy::from_agent_config(
        1, true, false, false, None,
    ));
    let mismatch_err = other
        .resume_from_checkpoint_in_place(&live, config(5), input("bad inplace"))
        .await
        .unwrap_err();
    assert!(
        mismatch_err.to_string().contains("match the source"),
        "in-place resume must reject a conflicting coordinator id: {mismatch_err}"
    );
}

/// The settle snapshot records how the run ended; resuming it must fail
/// loudly instead of silently re-driving a settled run.
#[tokio::test]
async fn resume_rejects_terminal_snapshot() {
    use wf_agent::error::AgentError;
    use wf_types::Id;

    let registry = registry_with_echo();
    let store = Arc::new(StorageBackend::new_memory());
    let (coordinator, mock) =
        interrupted_run("terminal-loop", store.clone(), registry.clone()).await;

    let (_, terminal) = resume_sources(&store, "terminal-loop").await;

    let in_place_err = coordinator
        .resume_from_checkpoint_in_place(&terminal, config(5), input("restart"))
        .await
        .unwrap_err();
    assert!(
        matches!(&in_place_err, AgentError::IllegalStateTransition(_)),
        "in-place resume must reject a terminal snapshot: {in_place_err}"
    );
    assert!(
        in_place_err.to_string().contains("terminal"),
        "the rejection must name the recorded status: {in_place_err}"
    );

    let branch = AgentLoopCoordinator::with_store(gateway_with(mock), registry, store.clone())
        .with_agent_loop_id(Id::from("terminal-branch"));
    let branch_err = branch
        .resume_from_checkpoint(&terminal, config(5), input("restart"))
        .await
        .unwrap_err();
    assert!(
        matches!(&branch_err, AgentError::IllegalStateTransition(_)),
        "branch resume must reject a terminal snapshot: {branch_err}"
    );
}
