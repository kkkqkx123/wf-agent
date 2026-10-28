use super::*;
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::BaseStorageAdapter;
use wf_storage::context::StorageContext;

fn gateway_with(mock: Arc<MockLlmClient>) -> Arc<LlmGateway> {
    let gateway = LlmGateway::new();
    gateway.register_mock("mock", mock);
    Arc::new(gateway)
}

fn make_ctx() -> Arc<ApiContext> {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("hello from mock"));
    let mut ctx = ApiContext::from_runtime_parts(
        Arc::new(StorageContext::new_memory()),
        Arc::new(ResourceRegistries::new()),
        Arc::new(wf_core::EventBus::new(64)),
        gateway_with(mock),
        Arc::new(wf_tools::create_default_tool_registry()),
        None,
    );
    ctx = ctx.with_checkpoint_store(Arc::new(wf_storage::backend::StorageBackend::new_memory()));
    Arc::new(ctx)
}

#[tokio::test]
async fn runs_agent_loop_to_completion() {
    let ctx = make_ctx();
    let output = run(
        &ctx,
        RunAgentLoopParams::new(
            AgentLoopConfig {
                file_scope: None,
                agent_id: wf_types::Id::from("agent-1".to_string()),
                model: "mock".to_string(),
                max_iterations: Some(3),
                max_execution_time: None,
                hooks: Vec::new(),
                available_tool_names: None,
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
            },
            AgentLoopInput {
                message: "hi".to_string(),
                context: Default::default(),
                conversation: Vec::new(),
            },
        ),
    )
    .await
    .expect("agent loop should complete");
    assert_eq!(output.result, serde_json::json!("hello from mock"));
    assert_eq!(output.iterations, 1);

    // The produced conversation is persisted and queryable via the
    // message adapter, scoped to the per-run agent loop id (not the agent
    // definition id).
    let persisted = ctx.storage.message.list(None).await.unwrap();
    assert!(
        !persisted.is_empty(),
        "agent conversation must be persisted"
    );
    let scoped = persisted[0].agent_loop_id.as_deref().unwrap();
    assert_ne!(scoped, "agent-1", "loop id must differ from definition id");
    assert!(persisted
        .iter()
        .all(|r| r.agent_loop_id.as_deref() == Some(scoped)));

    // An `AgentExecution` record is persisted, keyed by the
    // per-run agent loop id and linked to the definition.
    let executions = ctx.storage.agent_execution.list(None).await.unwrap();
    assert_eq!(executions.len(), 1, "agent execution must be persisted");
    assert_eq!(executions[0].definition_id, wf_types::Id::from("agent-1"));
    assert_eq!(executions[0].id.to_string(), scoped);
}

/// After a real `run`, the persisted `AgentExecution`
/// record is readable through a fresh context (empty live registries), so
/// the persisted branches of the agent queries return real data.
#[tokio::test]
async fn persisted_agent_execution_readable_after_restart() {
    let storage = Arc::new(StorageContext::new_memory());
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("persisted agent"));
    let gateway = gateway_with(mock);

    let mut ctx1 = ApiContext::from_runtime_parts(
        storage.clone(),
        Arc::new(ResourceRegistries::new()),
        Arc::new(wf_core::EventBus::new(64)),
        gateway.clone(),
        Arc::new(wf_tools::create_default_tool_registry()),
        None,
    );
    ctx1 = ctx1.with_checkpoint_store(Arc::new(wf_storage::backend::StorageBackend::new_memory()));

    let ctx1 = Arc::new(ctx1);
    let output = run(
        &ctx1,
        RunAgentLoopParams::new(
            AgentLoopConfig {
                file_scope: None,
                agent_id: wf_types::Id::from("agent-persist".to_string()),
                model: "mock".to_string(),
                max_iterations: Some(3),
                max_execution_time: None,
                hooks: Vec::new(),
                available_tool_names: None,
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
            },
            AgentLoopInput {
                message: "hi".to_string(),
                context: Default::default(),
                conversation: Vec::new(),
            },
        ),
    )
    .await
    .expect("agent loop should complete");
    let agent_loop_id = output.agent_loop_id.to_string();
    assert_ne!(agent_loop_id, "agent-persist");

    let ctx2 = Arc::new(ApiContext::from_runtime_parts(
        storage,
        Arc::new(ResourceRegistries::new()),
        Arc::new(wf_core::EventBus::new(64)),
        gateway,
        Arc::new(wf_tools::create_default_tool_registry()),
        None,
    ));

    let executions = crate::agent::list_agent_executions(&ctx2.storage, None)
        .await
        .unwrap();
    assert_eq!(executions.len(), 1);
    assert_eq!(
        executions[0].definition_id,
        wf_types::Id::from("agent-persist")
    );
    assert_eq!(executions[0].id.to_string(), agent_loop_id);
}

#[tokio::test]
async fn streams_agent_loop_events() {
    let ctx = make_ctx();
    let mut stream = stream(
        &ctx,
        RunAgentLoopParams::new(
            AgentLoopConfig {
                file_scope: None,
                agent_id: wf_types::Id::from("agent-2".to_string()),
                model: "mock".to_string(),
                max_iterations: Some(3),
                max_execution_time: None,
                hooks: Vec::new(),
                available_tool_names: None,
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
            },
            AgentLoopInput {
                message: "hi".to_string(),
                context: Default::default(),
                conversation: Vec::new(),
            },
        ),
    )
    .await
    .expect("agent stream");
    use futures::StreamExt;
    let mut saw_completed = false;
    while let Some(event) = stream.next().await {
        if let crate::infra::stream::ExecutionStreamEvent::Completed { .. } = event {
            saw_completed = true;
        }
    }
    assert!(saw_completed, "stream must end with Completed");
}

#[tokio::test]
async fn pause_resume_cancel_control_handles() {
    let ctx = make_ctx();
    let id = wf_types::Id::from("agent-ctrl-1".to_string());
    let entity = Arc::new(AgentLoopEntity::new(id.clone()));
    let _ = ctx.agent_loops.register(entity.clone());

    pause(&ctx, &id.to_string()).await.expect("pause");
    assert!(entity.state.read().await.is_paused());
    assert!(entity.interruption().is_interrupted());

    resume(&ctx, &id.to_string()).await.expect("resume");
    assert!(!entity.interruption().is_interrupted());

    cancel(&ctx, &id.to_string()).await.expect("cancel");
    assert!(entity.state.read().await.is_cancelled());

    let err = pause(&ctx, "missing").await.expect_err("unknown loop");
    assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
}

fn resume_config() -> AgentLoopConfig {
    AgentLoopConfig {
        file_scope: None,
        agent_id: wf_types::Id::from("agent-resume".to_string()),
        model: "mock".to_string(),
        max_iterations: Some(3),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: None,
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
        checkpoint_message_interval: Some(1),
    }
}

fn resume_input(message: &str) -> AgentLoopInput {
    AgentLoopInput {
        message: message.to_string(),
        context: Default::default(),
        conversation: Vec::new(),
    }
}

/// Run one agent loop and return `(loop_id, first_checkpoint_id)`.
///
/// Shared setup for the resume tests: each test needs its own completed
/// loop with at least one engine checkpoint, and keeping the setup here
/// (rather than repeating a long fixture) keeps the per-test futures
/// small. The resume tests each drive a full coordinator run, which is
/// the deepest stack user in this crate, so they stay split instead of
/// chaining several runs inside one test.
async fn run_loop_with_checkpoint(ctx: &Arc<ApiContext>, message: &str) -> (String, String) {
    use wf_checkpoint::CheckpointStateManager;

    let output = run(
        ctx,
        RunAgentLoopParams::new(resume_config(), resume_input(message)),
    )
    .await
    .expect("run completes");
    let loop_id = output.agent_loop_id.to_string();

    let state_manager =
        wf_checkpoint::AgentCheckpointStateManager::new(ctx.checkpoint_store.clone());
    let checkpoints = state_manager
        .list_by_entity(&loop_id)
        .await
        .expect("engine checkpoints list");
    assert!(
        !checkpoints.is_empty(),
        "message backstop must persist engine checkpoints"
    );
    (loop_id, checkpoints[0].id.to_string())
}

#[tokio::test]
async fn resume_from_engine_checkpoint_branches_under_new_execution_id() {
    let ctx = make_ctx();
    let (loop_id, checkpoint_id) = run_loop_with_checkpoint(&ctx, "hi").await;

    // Branch (default): a fresh execution id continues the restored
    // state; the source id is untouched.
    let branch = resume_from_checkpoint(
        &ctx,
        &loop_id,
        &checkpoint_id,
        RunAgentLoopParams::new(resume_config(), resume_input("branch off")),
        false,
    )
    .await
    .expect("branch resume completes");
    assert_ne!(
        branch.agent_loop_id.to_string(),
        loop_id,
        "branch resume must not reuse the source execution id"
    );
}

#[tokio::test]
async fn resume_from_engine_checkpoint_in_place_keeps_execution_id() {
    let ctx = make_ctx();
    let (loop_id, checkpoint_id) = run_loop_with_checkpoint(&ctx, "hi").await;

    // In-place: the completed source is terminal, so continuation under
    // the source id is accepted and appends to the same partition.
    let continued = resume_from_checkpoint(
        &ctx,
        &loop_id,
        &checkpoint_id,
        RunAgentLoopParams::new(resume_config(), resume_input("continue")),
        true,
    )
    .await
    .expect("in-place resume completes");
    assert_eq!(continued.agent_loop_id.to_string(), loop_id);
}

#[tokio::test]
async fn resume_rejects_foreign_and_unknown_checkpoints() {
    let ctx = make_ctx();
    let (loop_id, checkpoint_id) = run_loop_with_checkpoint(&ctx, "hi").await;

    // Ownership: a checkpoint of another loop is rejected.
    let (other_id, _) = run_loop_with_checkpoint(&ctx, "other").await;
    assert_ne!(other_id, loop_id);
    let err = resume_from_checkpoint(
        &ctx,
        &other_id,
        &checkpoint_id,
        RunAgentLoopParams::new(resume_config(), resume_input("cross")),
        true,
    )
    .await
    .expect_err("cross-loop checkpoint must be rejected");
    assert!(matches!(err, ApiError::Validation(_)));

    // An unknown checkpoint id is rejected too.
    let err = resume_from_checkpoint(
        &ctx,
        &loop_id,
        "missing-checkpoint",
        RunAgentLoopParams::new(resume_config(), resume_input("missing")),
        false,
    )
    .await
    .expect_err("unknown checkpoint must be rejected");
    assert!(matches!(err, ApiError::Validation(_)));
}
