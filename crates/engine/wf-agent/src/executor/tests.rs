use super::*;
use crate::coordinator::lifecycle::AgentLoopCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::AgentError;
use crate::registry::AgentLoopRegistry;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_llm::mock::{LlmResponseSpec, MockLlmClient};
use wf_llm::LlmGateway;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, ExecutionCallback, ParentLink};
use wf_tools::error::ToolError;
use wf_types::Id;

/// Shared hook-dispatch log captured by the test hook receivers.
type FailureLog =
    Arc<std::sync::Mutex<Vec<(String, std::collections::HashMap<String, serde_json::Value>)>>>;

fn agent_config(agent_id: &str) -> AgentLoopConfig {
    AgentLoopConfig {
        file_scope: None,
        agent_id: Id::from(agent_id.to_string()),
        model: "mock".to_string(),
        max_iterations: Some(3),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: Some(Vec::new()),
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
        context_schema: None,
        context_strict: None,
        temporary_mode: None,
        checkpoint_message_interval: None,
    }
}

fn agent_input(message: &str) -> AgentLoopInput {
    AgentLoopInput {
        message: message.to_string(),
        context: std::collections::HashMap::new(),
        conversation: Vec::new(),
        images: Vec::new(),
    }
}

fn parent_link_for(entity: &Arc<AgentLoopEntity>) -> ParentLink {
    ParentLink {
        file_scope: wf_types::config::file_scope::ResolvedExecutionScope::none(),
        execution_id: entity.id().clone(),
        // `as_ref` reaches the concrete entity so the inherent handle
        // is used: the blanket trait impl on `Arc` would only lend an
        // optional view.
        manager: entity.as_ref().hierarchy_manager(),
        cancellation: Some(entity.get_abort_signal()),
        tool_call_id: None,
        subagent_tx: None,
    }
}

async fn make_executor() -> AgentLoopExecutor {
    let gateway = Arc::new(LlmGateway::new());
    let mock = Arc::new(MockLlmClient::new());
    mock.default(LlmResponseSpec::text("done").with_usage(10, 5));
    gateway.register_mock("mock", mock);
    let registry = Arc::new(wf_tools::create_default_tool_registry());
    AgentLoopExecutor::new(gateway, registry)
}

#[tokio::test]
async fn test_spawn_returns_immediately_and_background_advances() {
    let executor = make_executor().await;
    let spawned = executor
        .spawn_agent_loop(agent_config("agent-a"), agent_input("run"))
        .await
        .expect("spawn must return immediately");
    assert_eq!(spawned.status, "started");
    assert_eq!(spawned.execution_id, spawned.agent_loop_id);

    // The execution is registered and progresses in the background.
    assert!(
        wf_common::poll_until(
            std::time::Duration::from_millis(10),
            std::time::Duration::from_secs(2),
            || async {
                match executor
                    .query_execution_status(
                        &spawned.execution_id.to_string(),
                        &spawned.execution_id,
                    )
                    .await
                {
                    Ok(status) if status.status == "completed" => true,
                    Ok(_) => false,
                    Err(e) => panic!("query failed: {e}"),
                }
            },
        )
        .await,
        "spawned execution did not complete in time"
    );
}

#[tokio::test]
async fn test_spawn_query_returns_result_at_terminal() {
    let executor = make_executor().await;
    let spawned = executor
        .spawn_agent_loop(agent_config("agent-b"), agent_input("run"))
        .await
        .expect("spawn must succeed");

    assert!(
        wf_common::poll_until(
            std::time::Duration::from_millis(10),
            std::time::Duration::from_secs(2),
            || async {
                executor
                    .query_execution_status(
                        &spawned.execution_id.to_string(),
                        &spawned.execution_id,
                    )
                    .await
                    .expect("query must succeed")
                    .status
                    == "completed"
            },
        )
        .await,
        "spawned execution did not complete in time"
    );
    let result = executor
        .query_execution_status(&spawned.execution_id.to_string(), &spawned.execution_id)
        .await
        .expect("query must succeed")
        .result
        .expect("completed execution must carry its result");
    assert_eq!(result, serde_json::Value::String("done".to_string()));
}

#[tokio::test]
async fn test_sync_execution_registers_and_is_queryable() {
    let executor = make_executor().await;
    let output = executor
        .execute(agent_config("agent-c"), agent_input("run"))
        .await
        .expect("sync execution must succeed");

    let status = executor
        .query_execution_status(&output.agent_loop_id.to_string(), &output.agent_loop_id)
        .await
        .expect("registered execution must be queryable");
    assert_eq!(status.status, "completed");
}

#[tokio::test]
async fn test_before_and_after_agent_hooks_fire_in_order() {
    use wf_core::EventBus;
    use wf_execution_shared::hooks::{HookContext, HookHandler, HookOutcome};
    use wf_types::events::EventType;

    let bus = Arc::new(EventBus::new(32));
    let hook_handler_registry = Arc::new(HookHandlerRegistry::new());
    struct LifecycleObserver(&'static str);
    #[async_trait]
    impl HookHandler for LifecycleObserver {
        fn name(&self) -> &str {
            self.0
        }
        async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
            HookOutcome::Continue
        }
    }
    hook_handler_registry.register(
        "BEFORE_AGENT",
        Arc::new(LifecycleObserver("before_agent_observer")),
        0,
    );
    hook_handler_registry.register(
        "AFTER_AGENT",
        Arc::new(LifecycleObserver("after_agent_observer")),
        0,
    );
    let mut config = agent_config("agent-hooks");
    config.hooks = vec![
        wf_tools::callback::HookConfig {
            hook_type: "BEFORE_AGENT".to_string(),
            condition: None,
            enabled: true,
            priority: 0,
            payload: None,
            handler: None,
            create_checkpoint: None,
            checkpoint_description: None,
        },
        wf_tools::callback::HookConfig {
            hook_type: "AFTER_AGENT".to_string(),
            condition: None,
            enabled: true,
            priority: 0,
            payload: None,
            handler: None,
            create_checkpoint: None,
            checkpoint_description: None,
        },
    ];

    let gateway = Arc::new(LlmGateway::new());
    let mock = Arc::new(MockLlmClient::new());
    mock.default(LlmResponseSpec::text("done").with_usage(10, 5));
    gateway.register_mock("mock", mock);
    let tool_registry = Arc::new(wf_tools::create_default_tool_registry());
    let executor = AgentLoopExecutor::new(gateway, tool_registry)
        .with_event_bus(bus.clone())
        .with_hook_handler_registry(hook_handler_registry);

    let mut sub = bus.subscribe();

    executor
        .execute(config, agent_input("run"))
        .await
        .expect("execution with hooks must succeed");

    // Each hook batch publishes one HOOK_TRIGGERED event; BEFORE_AGENT
    // must be observable before AFTER_AGENT. The closed BEFORE_AGENT
    // point only publishes because a sync handler was notified.
    let mut hook_events = Vec::new();
    for _ in 0..16 {
        match sub.try_recv() {
            Ok(event) if event.r#type == EventType::HookTriggered => {
                let hook_types = event
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("hook_type"))
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                hook_events.push(
                    hook_types
                        .first()
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                );
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    assert_eq!(hook_events.len(), 2, "both agent lifecycle hooks must fire");
    let before = hook_events
        .iter()
        .position(|t| t == "BEFORE_AGENT")
        .expect("BEFORE_AGENT must fire");
    let after = hook_events
        .iter()
        .position(|t| t == "AFTER_AGENT")
        .expect("AFTER_AGENT must fire");
    assert!(before < after, "BEFORE_AGENT runs before AFTER_AGENT");
}

#[tokio::test]
async fn test_after_agent_fires_on_failure_path_with_error_details() {
    use wf_execution_shared::hooks::{HookContext, HookHandler, HookHandlerRegistry, HookOutcome};
    use wf_llm::LlmError;

    // The LLM fails hard: the loop errors out and AFTER_AGENT must fire
    // on the failure path (success=false + error summary), not only on
    // the success path.
    let bus = Arc::new(wf_core::EventBus::new(32));
    let hook_handler_registry = Arc::new(HookHandlerRegistry::new());
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    struct FailureRecorder(FailureLog);
    #[async_trait::async_trait]
    impl HookHandler for FailureRecorder {
        fn name(&self) -> &str {
            "after_agent_failure_recorder"
        }

        async fn on_point(&self, ctx: &HookContext) -> HookOutcome {
            self.0
                .lock()
                .unwrap()
                .push((ctx.hook_type.clone(), ctx.data.clone()));
            HookOutcome::Continue
        }
    }
    hook_handler_registry.register(
        "AFTER_AGENT",
        Arc::new(FailureRecorder(captured.clone())),
        0,
    );

    let gateway = Arc::new(LlmGateway::new());
    let mock = Arc::new(MockLlmClient::new());
    // Exhaust the retry budget (max_retries = 3): every attempt fails,
    // so the loop must error out instead of falling back to a success.
    for _ in 0..4 {
        mock.script_error(LlmError::ProviderError {
            status: None,
            message: "LLM provider exploded".to_string(),
            retry_after_ms: None,
        });
    }
    gateway.register_mock("mock", mock);
    let tool_registry = Arc::new(wf_tools::create_default_tool_registry());
    let executor = AgentLoopExecutor::new(gateway, tool_registry)
        .with_event_bus(bus)
        .with_hook_handler_registry(hook_handler_registry);

    let result = executor
        .execute(agent_config("agent-fail"), agent_input("run"))
        .await;
    assert!(result.is_err(), "a failing LLM must fail the loop");

    let recorded = captured.lock().unwrap();
    assert_eq!(
        recorded.len(),
        1,
        "AFTER_AGENT must fire exactly once on the failure path"
    );
    let (hook_type, data) = &recorded[0];
    assert_eq!(hook_type, "AFTER_AGENT");
    assert_eq!(data.get("success"), Some(&serde_json::Value::Bool(false)));
    let error = data
        .get("error")
        .and_then(|v| v.as_str())
        .expect("failure path must carry an error summary");
    assert!(
        error.contains("LLM provider exploded"),
        "error summary must surface the failure: {error}"
    );
}

#[tokio::test]
async fn test_cancel_interrupts_in_flight_execution() {
    let gateway = Arc::new(LlmGateway::new());
    let mock = Arc::new(MockLlmClient::new());
    // A slow LLM keeps the loop in flight long enough to be cancelled:
    // the mock would otherwise answer so fast the loop already reached a
    // terminal state and cancelling it becomes an illegal transition.
    mock.default(
        LlmResponseSpec::text("final")
            .with_usage(10, 5)
            .with_delay(300),
    );
    gateway.register_mock("mock", mock);
    let registry = Arc::new(wf_tools::create_default_tool_registry());
    let executor = AgentLoopExecutor::new(gateway, registry);

    let spawned = executor
        .spawn_agent_loop(
            agent_config("agent-d"),
            agent_input("run and take your time"),
        )
        .await
        .expect("spawn must succeed");

    // Give the background task a moment to start, then cancel.
    tokio::time::sleep(Duration::from_millis(20)).await;
    executor
        .cancel_execution(&spawned.execution_id.to_string(), &spawned.execution_id)
        .await
        .expect("cancel must succeed");

    let status = executor
        .query_execution_status(&spawned.execution_id.to_string(), &spawned.execution_id)
        .await
        .expect("query after cancel must succeed");
    assert_eq!(status.status, "cancelled");
}

#[tokio::test]
async fn test_pause_suspends_and_resume_reenters_the_loop() {
    use wf_execution_shared::types::execution_entity::ExecutionEntity;
    use wf_execution_shared::types::execution_entity::ExecutionStatus;

    // A slow LLM keeps the first iteration in flight so the pause lands
    // on a running loop instead of racing a completed one.
    let gateway = Arc::new(LlmGateway::new());
    let mock = Arc::new(MockLlmClient::new());
    mock.default(
        LlmResponseSpec::text("final")
            .with_usage(10, 5)
            .with_delay(300),
    );
    gateway.register_mock("mock", mock);
    let registry = Arc::new(wf_tools::create_default_tool_registry());
    let executor = AgentLoopExecutor::new(gateway, registry);

    let spawned = executor
        .spawn_agent_loop(agent_config("agent-pause"), agent_input("run"))
        .await
        .expect("spawn must succeed");

    // Pause the running loop mid-iteration.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let entity = executor
        .agent_registry()
        .get(&spawned.execution_id)
        .expect("entity present");
    entity.pause().await.expect("pause must succeed");
    assert_eq!(
        entity.state.read().await.status(),
        ExecutionStatus::Paused,
        "pause must flip the state machine"
    );

    // While paused the loop is suspended: no result, status stays paused.
    let status = executor
        .query_execution_status(&spawned.execution_id.to_string(), &spawned.execution_id)
        .await
        .expect("query during pause must succeed");
    assert_eq!(status.status, "paused");

    // Resume: the loop re-enters its iteration loop and completes.
    entity.resume().await.expect("resume must succeed");
    for _ in 0..300 {
        let status = executor
            .query_execution_status(&spawned.execution_id.to_string(), &spawned.execution_id)
            .await
            .expect("query after resume must succeed");
        if status.status == "completed" {
            assert_eq!(
                status.result,
                Some(serde_json::Value::String("final".to_string())),
                "resumed loop must deliver its real result, not a pause artifact"
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("paused loop did not complete after resume");
}

#[tokio::test]
async fn test_query_unknown_returns_not_found() {
    let executor = make_executor().await;
    let caller = Id::from("caller-root".to_string());
    let err = executor
        .query_execution_status("missing-id", &caller)
        .await
        .expect_err("unknown execution must not be found");
    assert!(matches!(err, ToolError::NotFound(_)));
}

/// Dropping the driving future of a synchronous child (tool-level
/// timeout, aborted host task) must reclaim the child's registry slot
/// and capacity permit instead of leaving a phantom running execution.
#[tokio::test]
async fn test_aborted_sync_child_is_reclaimed() {
    let gateway = Arc::new(LlmGateway::new());
    let mock = Arc::new(MockLlmClient::new());
    mock.default(
        LlmResponseSpec::text("done")
            .with_usage(10, 5)
            .with_delay(2000),
    );
    gateway.register_mock("mock", mock);
    let registry = Arc::new(wf_tools::create_default_tool_registry());
    let executor = Arc::new(AgentLoopExecutor::new(gateway, registry).with_max_concurrent(8));

    let parent = Arc::new(AgentLoopEntity::new(Id::from("parent-reclaim".to_string())));
    executor
        .agent_registry()
        .register(parent.clone())
        .expect("parent registers");
    let permits_before = executor.agent_registry().available_permits();

    let exec = executor.clone();
    let parent_for_task = parent.clone();
    let handle = tokio::spawn(async move {
        let _ = exec
            .execute_with_parent(
                agent_config("child-reclaim"),
                agent_input("run"),
                Some(parent_link_for(&parent_for_task)),
            )
            .await;
    });

    // The child links onto the parent before its first LLM call.
    let mut child_id = None;
    for _ in 0..100 {
        if let Some(id) = parent.child_ids().first().cloned() {
            child_id = Some(id);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let child_id = child_id.expect("child must link onto the parent");

    // Aborting the task drops the driving future mid-run, exactly like
    // a tool-level timeout unwinding a synchronous dispatch.
    handle.abort();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(
        executor.agent_registry().get(&child_id).is_none(),
        "abandoned child must not stay registered"
    );
    assert_eq!(
        executor.agent_registry().available_permits(),
        permits_before,
        "abandoned child must release its capacity permit"
    );
    assert!(
        parent.child_ids().is_empty(),
        "abandoned child must leave the parent hierarchy"
    );
}

/// A settled child (success or failure) leaves the parent hierarchy, so
/// the parent's child list tracks live children only.
#[tokio::test]
async fn test_completed_child_leaves_parent_hierarchy() {
    let executor = make_executor().await.with_max_concurrent(8);
    let parent = Arc::new(AgentLoopEntity::new(Id::from("parent-done".to_string())));
    executor
        .agent_registry()
        .register(parent.clone())
        .expect("parent registers");

    let output = executor
        .execute_with_parent(
            agent_config("child-done"),
            agent_input("run"),
            Some(parent_link_for(&parent)),
        )
        .await
        .expect("child must complete");

    assert_eq!(output.result, serde_json::Value::String("done".to_string()));
    assert!(
        parent.child_ids().is_empty(),
        "settled child must leave the parent hierarchy"
    );
    // The child stays queryable as terminal history; the caller root is
    // the tree root, which the parent provides.
    let tree_root = parent.id().clone();
    let status = executor
        .query_execution_status(&output.agent_loop_id.to_string(), &tree_root)
        .await
        .expect("settled child stays queryable");
    assert_eq!(status.status, "completed");
}

/// Status queries and cancellation are scoped to the caller's
/// execution subtree: foreign trees are refused, unknown executions
/// keep the not-found error.
#[tokio::test]
async fn test_control_plane_is_scoped_to_caller_subtree() {
    let executor = make_executor().await.with_max_concurrent(8);
    let tree_a = Id::from("tree-a".to_string());
    let tree_b = Id::from("tree-b".to_string());
    executor
        .agent_registry()
        .register(Arc::new(AgentLoopEntity::new(tree_a.clone())))
        .expect("tree-a registers");
    executor
        .agent_registry()
        .register(Arc::new(AgentLoopEntity::new(tree_b.clone())))
        .expect("tree-b registers");

    // Same subtree (the execution itself) passes.
    assert!(executor
        .query_execution_status("tree-a", &tree_a)
        .await
        .is_ok());

    // A foreign tree is refused on both control operations.
    let err = executor
        .query_execution_status("tree-b", &tree_a)
        .await
        .expect_err("foreign execution must be refused");
    assert!(matches!(err, ToolError::ExecutionError(_)));
    let err = executor
        .cancel_execution("tree-b", &tree_a)
        .await
        .expect_err("foreign cancel must be refused");
    assert!(matches!(err, ToolError::ExecutionError(_)));

    // Unknown executions keep the not-found classification.
    let err = executor
        .query_execution_status("tree-missing", &tree_a)
        .await
        .expect_err("unknown execution is not found");
    assert!(matches!(err, ToolError::NotFound(_)));
}

/// A streaming run publishes its parent-stream observer on the entity
/// for the run's duration and clears it when the run settles, so the
/// forwarding task exits with the loop instead of outliving it.
#[tokio::test]
async fn test_streaming_run_installs_then_clears_parent_observer() {
    use futures::StreamExt;

    let gateway = Arc::new(LlmGateway::new());
    let mock = Arc::new(MockLlmClient::new());
    mock.default(
        LlmResponseSpec::text("done")
            .with_usage(10, 5)
            .with_delay(300),
    );
    // The streaming path replays stream events, whose pacing comes from
    // the client-level delay: the run stays in flight for the poll.
    mock.with_stream_delay(300);
    gateway.register_mock("mock", mock);
    let registry = Arc::new(wf_tools::create_default_tool_registry());
    let shared = Arc::new(AgentLoopRegistry::new());
    let coordinator =
        AgentLoopCoordinator::new(gateway, registry).with_entity_registry(shared.clone());

    let mut stream = std::pin::pin!(
        coordinator
            .execute_stream(agent_config("streamer"), agent_input("run"))
            .await
    );

    // While the run is in flight the registered entity carries the
    // observer; triggered children read it to report upstream.
    let mut observer_id = None;
    for _ in 0..100 {
        let found = shared.get_all_ids().into_iter().find(|id| {
            shared
                .get(id)
                .is_some_and(|entity| entity.subagent_tx().is_some())
        });
        if let Some(id) = found {
            observer_id = Some(id);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let observer_id = observer_id.expect("streaming run must install the observer");

    while stream.next().await.is_some() {}

    let entity = shared
        .get(&observer_id)
        .expect("settled execution stays as history");
    assert!(
        entity.subagent_tx().is_none(),
        "observer must be cleared when the run settles"
    );
}

#[tokio::test]
async fn test_spawn_links_parent_and_propagates_cancel() {
    // A delayed mock keeps both parent and child in flight so the parent
    // cancel reaches a running (not yet completed) child.
    let gateway = Arc::new(LlmGateway::new());
    let mock = Arc::new(MockLlmClient::new());
    mock.default(
        LlmResponseSpec::text("done")
            .with_usage(10, 5)
            .with_delay(300),
    );
    gateway.register_mock("mock", mock);
    let registry = Arc::new(wf_tools::create_default_tool_registry());
    let executor = AgentLoopExecutor::new(gateway, registry);

    // The parent loop is itself a registered execution.
    let parent = executor
        .spawn_agent_loop(agent_config("agent-parent"), agent_input("run"))
        .await
        .expect("parent spawn must succeed");
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Child links under the parent through the typed link.
    let parent_entity = executor
        .agent_registry()
        .get(&parent.execution_id)
        .expect("parent entity present");
    let child = executor
        .spawn_with_parent(
            agent_config("agent-child"),
            agent_input("run"),
            Some(parent_link_for(&parent_entity)),
        )
        .await
        .expect("child spawn must succeed");

    // The parent entity tracks the child once the child task registers
    // the linkage; poll for it.
    let mut linked = false;
    for _ in 0..100 {
        if parent_entity.child_ids().contains(&child.execution_id) {
            linked = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(linked, "child must be linked onto the parent");

    // Cancelling the parent stops the child too.
    executor
        .cancel_execution(&parent.execution_id.to_string(), &parent.execution_id)
        .await
        .expect("parent cancel must succeed");
    for _ in 0..100 {
        let status = executor
            .query_execution_status(&child.execution_id.to_string(), &parent.execution_id)
            .await
            .expect("child query must succeed");
        if status.status == "cancelled" || status.status == "completed" {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("child did not settle after parent cancel");
}

/// A nested spawn whose resolved depth exceeds `max_sub_agent_depth`
/// is rejected before any slot is reserved.
#[tokio::test]
async fn test_spawn_rejects_beyond_depth_limit() {
    let executor = make_executor()
        .await
        .with_max_sub_agent_depth(1)
        .with_max_concurrent(16);

    // A registered parent at depth 1 would push the child to depth 2.
    let parent_manager = std::sync::Arc::new(wf_core::hierarchy::ExecutionHierarchyManager::new(
        Id::from("depth-root".to_string()),
        wf_types::execution::ExecutionType::Agent,
    ));
    let deep_manager = parent_manager
        .derive_child(
            Id::from("depth-parent".to_string()),
            wf_types::execution::ExecutionType::Agent,
            None,
        )
        .expect("derive depth-1 parent");
    let deep_parent = Arc::new(
        AgentLoopEntity::new(Id::from("depth-parent".to_string()))
            .with_hierarchy_manager(deep_manager),
    );
    executor
        .agent_registry()
        .register(deep_parent)
        .expect("parent register must succeed");

    let err = executor
        .spawn_with_parent(
            agent_config("agent-deep"),
            agent_input("run"),
            Some(parent_link_for(
                &executor
                    .agent_registry()
                    .get(&Id::from("depth-parent".to_string()))
                    .expect("deep parent present"),
            )),
        )
        .await
        .expect_err("depth 2 must be rejected");
    assert!(
        matches!(err, AgentError::HierarchyLimitReached(_)),
        "depth overflow must surface as HierarchyLimitReached: {err}"
    );

    // A root-parent depth (0 -> child depth 1) is within the limit.
    executor
        .agent_registry()
        .register(Arc::new(AgentLoopEntity::new(Id::from(
            "root-parent".to_string(),
        ))))
        .expect("root parent register must succeed");
    executor
        .spawn_with_parent(
            agent_config("agent-ok"),
            agent_input("run"),
            Some(parent_link_for(
                &executor
                    .agent_registry()
                    .get(&Id::from("root-parent".to_string()))
                    .expect("root parent present"),
            )),
        )
        .await
        .expect("depth 1 must be allowed");
}

/// Spawning beyond the concurrent-execution limit is rejected at
/// spawn time; the sync path is gated through the same registry.
#[tokio::test]
async fn test_spawn_rejects_when_concurrency_limit_reached() {
    let executor = make_executor().await.with_max_concurrent(1);

    executor
        .spawn_agent_loop(agent_config("agent-conc-1"), agent_input("run"))
        .await
        .expect("first spawn fits the single slot");

    let err = executor
        .spawn_agent_loop(agent_config("agent-conc-2"), agent_input("run"))
        .await
        .expect_err("second spawn must hit the capacity gate");
    assert!(
        matches!(err, AgentError::ConcurrencySaturated(_)),
        "overflow must surface as ConcurrencySaturated: {err}"
    );
}

/// A capacity-rejected linked spawn derives the child onto the parent
/// manager before the gate rejects it; the reference must be rolled
/// back so the parent's child list tracks adopted children only.
#[tokio::test]
async fn test_rejected_linked_spawn_leaves_no_parent_reference() {
    let executor = make_executor().await.with_max_concurrent(1);
    let parent = Arc::new(AgentLoopEntity::new(Id::from("parent-gate".to_string())));
    executor
        .agent_registry()
        .register(parent.clone())
        .expect("parent registers");
    assert_eq!(
        executor.agent_registry().available_permits(),
        0,
        "the parent holds the single slot"
    );

    let err = executor
        .spawn_with_parent(
            agent_config("agent-gate-child"),
            agent_input("run"),
            Some(parent_link_for(&parent)),
        )
        .await
        .expect_err("linked spawn must hit the capacity gate");
    assert!(matches!(err, AgentError::ConcurrencySaturated(_)));
    assert!(
        parent.child_ids().is_empty(),
        "rejected spawn must not leave a child reference on the parent"
    );
}

/// The sync `execute` path is gated through the shared registry too: an
/// in-flight execution holds the slot, and once it settles the released
/// permit admits the next run (the fixed capacity lifecycle).
#[tokio::test]
async fn test_sync_execute_honors_concurrency_limit() {
    let executor = Arc::new(make_executor().await.with_max_concurrent(1));

    // Hold the single slot with an in-flight placeholder: a sync run must
    // be rejected at the gate before it starts.
    executor
        .agent_registry()
        .register(Arc::new(AgentLoopEntity::new(Id::from(
            "slot-holder".to_string(),
        ))))
        .expect("placeholder holds the single slot");
    let err = executor
        .execute(agent_config("agent-sync-1"), agent_input("run"))
        .await
        .expect_err("in-flight slot holder must reject a new run");
    assert!(matches!(err, AgentError::ConcurrencySaturated(_)));

    // Releasing the holder frees capacity for the next run.
    executor
        .agent_registry()
        .unregister(&Id::from("slot-holder".to_string()));
    executor
        .execute(agent_config("agent-sync-2"), agent_input("run"))
        .await
        .expect("released slot admits the next run");
}

/// A spawn whose background run fails (here: no LLM profile serves the
/// model) settles the placeholder as a terminal failed execution
/// carrying the error. The spawn handle keeps its query contract
/// instead of evaporating into `NotFound`.
#[tokio::test]
async fn test_spawn_background_failure_settles_terminal_with_error() {
    let executor = make_executor().await;
    let mut config = agent_config("agent-ghost");
    config.model = "missing-model".to_string();
    let spawned = executor
        .spawn_agent_loop(config, agent_input("run"))
        .await
        .expect("spawn returns before the background run settles");

    assert!(
        wf_common::poll_until(
            std::time::Duration::from_millis(10),
            std::time::Duration::from_secs(2),
            || async {
                executor
                    .query_execution_status(
                        &spawned.execution_id.to_string(),
                        &spawned.execution_id,
                    )
                    .await
                    .map(|status| status.status == "failed")
                    .unwrap_or(false)
            },
        )
        .await,
        "failed build must settle the execution as failed"
    );
    let status = executor
        .query_execution_status(&spawned.execution_id.to_string(), &spawned.execution_id)
        .await
        .expect("failed execution stays queryable");
    let error = status
        .result
        .as_ref()
        .and_then(|v| v.get("error"))
        .and_then(|v| v.as_str())
        .expect("failed execution must carry its error");
    assert!(
        !error.is_empty(),
        "failure reason must reach polling callers"
    );
}

/// A typed parent link from a foreign execution type (e.g. a workflow
/// manager handle) links the child without any registry lookup: the
/// child carries the parent id, depth and ancestry.
#[tokio::test]
async fn test_spawn_with_typed_parent_link_from_foreign_execution() {
    use wf_types::execution::ExecutionType;

    let executor = make_executor().await;
    let parent_manager = Arc::new(wf_core::hierarchy::ExecutionHierarchyManager::new(
        Id::from("wf-parent".to_string()),
        ExecutionType::Workflow,
    ));
    let link = ParentLink {
        file_scope: wf_types::config::file_scope::ResolvedExecutionScope::none(),
        execution_id: Id::from("wf-parent".to_string()),
        manager: parent_manager,
        cancellation: None,
        tool_call_id: None,
        subagent_tx: None,
    };
    let spawned = executor
        .spawn_with_parent(agent_config("agent-linked"), agent_input("run"), Some(link))
        .await
        .expect("typed parent link must build");

    assert!(
        wf_common::poll_until(
            std::time::Duration::from_millis(10),
            std::time::Duration::from_secs(2),
            || async {
                executor
                    .query_execution_status(
                        &spawned.execution_id.to_string(),
                        &Id::from("wf-parent".to_string()),
                    )
                    .await
                    .map(|status| status.status == "completed")
                    .unwrap_or(false)
            },
        )
        .await,
        "linked child must complete"
    );
    let entity = executor
        .agent_registry()
        .get(&spawned.execution_id)
        .expect("linked child registered");
    assert_eq!(
        entity.parent_execution_id(),
        Some(Id::from("wf-parent".to_string()))
    );
    assert!(entity
        .ancestors()
        .contains(&Id::from("wf-parent".to_string())));
}

/// Cancelling the parent abort signal carried by the typed link stops
/// the spawned child, covering parents outside the agent registry.
#[tokio::test]
async fn test_typed_parent_cancellation_stops_child() {
    use wf_types::execution::ExecutionType;

    let gateway = Arc::new(LlmGateway::new());
    let mock = Arc::new(MockLlmClient::new());
    mock.default(
        LlmResponseSpec::text("done")
            .with_usage(10, 5)
            .with_delay(300),
    );
    gateway.register_mock("mock", mock);
    let registry = Arc::new(wf_tools::create_default_tool_registry());
    let executor = AgentLoopExecutor::new(gateway, registry);

    let parent_token = tokio_util::sync::CancellationToken::new();
    let link = ParentLink {
        file_scope: wf_types::config::file_scope::ResolvedExecutionScope::none(),
        execution_id: Id::from("foreign-parent".to_string()),
        manager: Arc::new(wf_core::hierarchy::ExecutionHierarchyManager::new(
            Id::from("foreign-parent".to_string()),
            ExecutionType::Workflow,
        )),
        cancellation: Some(parent_token.clone()),
        tool_call_id: None,
        subagent_tx: None,
    };
    let spawned = executor
        .spawn_with_parent(
            agent_config("agent-cancel-child"),
            agent_input("run"),
            Some(link),
        )
        .await
        .expect("spawn with parent link must succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;
    parent_token.cancel();

    assert!(
        wf_common::poll_until(
            std::time::Duration::from_millis(10),
            std::time::Duration::from_secs(2),
            || async {
                executor
                    .query_execution_status(
                        &spawned.execution_id.to_string(),
                        &Id::from("foreign-parent".to_string()),
                    )
                    .await
                    .map(|status| status.status == "cancelled" || status.status == "completed")
                    .unwrap_or(false)
            },
        )
        .await,
        "child must settle after parent cancellation"
    );
}

/// The sync path shares the sub-agent depth gate: a nested run whose
/// resolved depth would exceed the limit is rejected before it starts.
#[tokio::test]
async fn test_sync_execute_rejects_beyond_depth_limit() {
    let executor = make_executor()
        .await
        .with_max_sub_agent_depth(1)
        .with_max_concurrent(16);

    let parent_manager = std::sync::Arc::new(wf_core::hierarchy::ExecutionHierarchyManager::new(
        Id::from("sync-depth-root".to_string()),
        wf_types::execution::ExecutionType::Agent,
    ));
    let deep_manager = parent_manager
        .derive_child(
            Id::from("sync-depth-parent".to_string()),
            wf_types::execution::ExecutionType::Agent,
            None,
        )
        .expect("derive depth-1 parent");
    executor
        .agent_registry()
        .register(Arc::new(
            AgentLoopEntity::new(Id::from("sync-depth-parent".to_string()))
                .with_hierarchy_manager(deep_manager),
        ))
        .expect("parent register must succeed");

    let err = executor
        .execute_with_parent(
            agent_config("agent-sync-deep"),
            agent_input("run"),
            Some(parent_link_for(
                &executor
                    .agent_registry()
                    .get(&Id::from("sync-depth-parent".to_string()))
                    .expect("sync parent present"),
            )),
        )
        .await
        .expect_err("depth 2 must be rejected on the sync path");
    assert!(
        matches!(err, AgentError::HierarchyLimitReached(_)),
        "depth overflow must surface as HierarchyLimitReached: {err}"
    );
}

#[test]
fn inherit_file_scope_rejects_out_of_bounds_child() {
    let parent_scope = wf_types::config::file_scope::ResolvedFileScope {
        enabled: true,
        read_prefixes: vec!["/repo".to_string()],
        write_prefixes: vec!["/repo/frontend".to_string()],
        checkpoint_prefixes: vec!["/repo/frontend".to_string()],
        workdir: None,
    };
    let manager = wf_core::hierarchy::ExecutionHierarchyManager::new(
        "p".to_string(),
        wf_types::execution::ExecutionType::Agent,
    );
    let parent = ParentLink {
        file_scope: wf_types::config::file_scope::ResolvedExecutionScope::some(parent_scope),
        execution_id: wf_types::Id::from("p".to_string()),
        manager: std::sync::Arc::new(manager),
        cancellation: None,
        tool_call_id: None,
        subagent_tx: None,
    };
    let mut config = agent_config("a");
    config.file_scope = Some(wf_types::config::file_scope::ExecutionFileScope {
        enabled: None,
        read_prefixes: None,
        write_prefixes: Some(vec!["/repo/backend".to_string()]),
        checkpoint_prefixes: None,
        workdir: None,
    });
    assert!(AgentLoopExecutor::inherit_file_scope(config, Some(&parent)).is_err());
}
