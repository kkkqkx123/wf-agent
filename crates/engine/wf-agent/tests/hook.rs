//! Hook checkpoint integration tests (`hook::AgentHookEmitter` +
//! `checkpoint::AgentCheckpointIntegration`): the `create_checkpoint` opt-in
//! gate across all wired hook points, isolated and end-to-end.

use std::sync::Arc;

use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_storage::backend::StorageBackend;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_tools::registry::ToolRegistry;

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

async fn fire_hook_with_opt_in(
    store: Arc<StorageBackend>,
    loop_id: &str,
    hook_type: &str,
    create_checkpoint: Option<bool>,
) -> u64 {
    use wf_agent::checkpoint::AgentCheckpointIntegration;
    use wf_agent::entity::AgentLoopEntity;
    use wf_agent::hook::AgentHookEmitter;
    use wf_agent::AgentCheckpointStrategy;
    use wf_checkpoint::state::agent::AgentCheckpointStateManager;
    use wf_checkpoint::state::CheckpointStateManager;
    use wf_execution_shared::hooks::types::HookDefinition;
    use wf_types::checkpoint::{CheckpointTiming, UnifiedCheckpointPolicy};

    let entity =
        AgentLoopEntity::new(wf_types::Id::from(loop_id)).with_hooks(vec![HookDefinition {
            id: "hook-1".to_string(),
            hook_type: hook_type.to_string(),
            priority: 0,
            condition: None,
            enabled: true,
            payload: None,
            handler: None,
            create_checkpoint,
            checkpoint_description: None,
        }]);
    let cp = AgentCheckpointIntegration::new(store.clone()).with_strategy(
        AgentCheckpointStrategy::from_policy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![
                CheckpointTiming::BeforeExecute,
                CheckpointTiming::AfterExecute,
                CheckpointTiming::ToolBefore,
                CheckpointTiming::ToolAfter,
                CheckpointTiming::Manual,
                CheckpointTiming::OnComplete,
            ],
            content: None,
            retention: None,
            error_handling: None,
        }),
    );
    AgentHookEmitter::fire_agent_point_with_checkpoint(
        &entity,
        hook_type,
        std::collections::HashMap::new(),
        None,
        None,
        Some(&cp),
    )
    .await;
    AgentCheckpointStateManager::new(store)
        .count_by_entity(loop_id)
        .await
        .unwrap()
}

#[tokio::test]
async fn hook_create_checkpoint_persists_after_iteration() {
    let store = Arc::new(StorageBackend::new_memory());
    assert_eq!(
        fire_hook_with_opt_in(store.clone(), "hook-fire", "AFTER_ITERATION", Some(true)).await,
        1,
        "opted-in hook fire must persist exactly one hook-requested checkpoint"
    );
    let plain_store = Arc::new(StorageBackend::new_memory());
    assert_eq!(
        fire_hook_with_opt_in(plain_store, "hook-quiet", "AFTER_ITERATION", None).await,
        0,
        "hook fire without the opt-in must persist nothing"
    );

    let e2e_store = Arc::new(StorageBackend::new_memory());
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("done"));
    let coordinator = AgentLoopCoordinator::with_store(
        gateway_with(mock),
        registry_with_echo(),
        e2e_store.clone(),
    )
    .with_agent_loop_id(wf_types::Id::from("hook-loop"))
    .with_checkpoint_strategy(wf_agent::AgentCheckpointStrategy::every_iteration());
    let mut cfg = config(5);
    cfg.hooks.push(wf_tools::callback::HookConfig {
        hook_type: "AFTER_ITERATION".to_string(),
        condition: None,
        enabled: true,
        priority: 0,
        payload: None,
        handler: None,
        create_checkpoint: Some(true),
        checkpoint_description: None,
    });
    let output = coordinator.execute(cfg, input("hook run")).await.unwrap();
    assert_eq!(output.agent_loop_id, "hook-loop");
    assert!(
        {
            use wf_checkpoint::state::agent::AgentCheckpointStateManager;
            use wf_checkpoint::state::CheckpointStateManager;
            AgentCheckpointStateManager::new(e2e_store)
                .count_by_entity("hook-loop")
                .await
                .unwrap()
        } >= 1,
        "hook-requested checkpoint must persist after iteration"
    );
}

#[tokio::test]
async fn hook_create_checkpoint_covers_all_wired_points() {
    for hook_type in [
        "BEFORE_ITERATION",
        "BEFORE_LLM_CALL",
        "AFTER_LLM_CALL",
        "AFTER_ITERATION",
        "BEFORE_TOOL_CALL",
        "AFTER_TOOL_CALL",
        "BEFORE_USER_PROMPT",
        "BEFORE_AGENT",
        "AFTER_AGENT",
    ] {
        let store = Arc::new(StorageBackend::new_memory());
        assert_eq!(
            fire_hook_with_opt_in(store.clone(), "wired-on", hook_type, Some(true)).await,
            1,
            "opted-in {hook_type} fire must persist exactly one hook-requested checkpoint"
        );
        let quiet = Arc::new(StorageBackend::new_memory());
        assert_eq!(
            fire_hook_with_opt_in(quiet, "wired-off", hook_type, None).await,
            0,
            "{hook_type} fire without the opt-in must persist nothing"
        );
    }

    {
        use wf_agent::checkpoint::AgentCheckpointIntegration;
        use wf_agent::entity::AgentLoopEntity;
        use wf_agent::hook::AgentHookEmitter;
        use wf_agent::AgentCheckpointStrategy;
        use wf_checkpoint::state::agent::AgentCheckpointStateManager;
        use wf_checkpoint::state::CheckpointStateManager;
        use wf_execution_shared::hooks::types::HookDefinition;

        let store = Arc::new(StorageBackend::new_memory());
        let entity = AgentLoopEntity::new(wf_types::Id::from("gated-off")).with_hooks(vec![
            HookDefinition {
                id: "hook-1".to_string(),
                hook_type: "AFTER_ITERATION".to_string(),
                priority: 0,
                condition: None,
                enabled: true,
                payload: None,
                handler: None,
                create_checkpoint: Some(true),
                checkpoint_description: None,
            },
        ]);
        let cp = AgentCheckpointIntegration::new(store.clone())
            .with_strategy(AgentCheckpointStrategy::never());
        AgentHookEmitter::fire_agent_point_with_checkpoint(
            &entity,
            "AFTER_ITERATION",
            std::collections::HashMap::new(),
            None,
            None,
            Some(&cp),
        )
        .await;
        assert_eq!(
            AgentCheckpointStateManager::new(store)
                .count_by_entity("gated-off")
                .await
                .unwrap(),
            0,
            "disabled strategy must suppress even an opted-in hook fire"
        );
    }
}
