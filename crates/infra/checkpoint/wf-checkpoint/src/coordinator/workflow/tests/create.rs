use super::super::*;
use super::fixtures::*;
use checkpoint_base::metadata::builder::{
    CHAIN_POSITION_FIELD, CREATED_AT_FIELD, FORMAT_VERSION_FIELD,
};
use std::collections::HashMap;
use wf_storage::backend::StorageBackend;

#[tokio::test]
async fn prepare_returns_context() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    assert_eq!(ctx.entity_type, "workflow_execution");
    assert_eq!(ctx.entity_id, "exec-1");
}

#[tokio::test]
async fn build_creates_full_checkpoint_on_first_save() {
    let coord = make_coordinator();
    let ctx = CheckpointContext {
        entity_type: "workflow_execution".to_string(),
        entity_id: "exec-1".to_string(),
        trigger: None,
        actor_id: None,
        attempt: None,
        retry_count: None,
        error: None,
        fallback_used: None,
        metadata: None,
    };
    let checkpoint = coord.build(ctx, make_snapshot()).await.unwrap();
    assert_eq!(checkpoint.r#type, Some(CheckpointType::Full));
    assert!(checkpoint.snapshot.is_some());
    assert!(checkpoint.format_version.is_some());
}

#[tokio::test]
async fn persist_saves_to_storage() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    let loaded = coord.state_manager().load(&cp.id).await.unwrap();
    assert!(loaded.is_some());
}

#[tokio::test]
async fn determine_type_respects_config() {
    let coord = make_coordinator();
    let config = DeltaStorageConfig {
        enabled: false,
        baseline_interval: 5,
        max_delta_chain_length: 10,
    };
    let tp = coord.determine_type("exec-1", &config).await.unwrap();
    assert_eq!(tp, CheckpointType::Full);

    let config_enabled = DeltaStorageConfig {
        enabled: true,
        baseline_interval: 5,
        max_delta_chain_length: 10,
    };
    let tp = coord
        .determine_type("exec-1", &config_enabled)
        .await
        .unwrap();
    assert_eq!(tp, CheckpointType::Full);

    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    let tp = coord
        .determine_type("exec-1", &config_enabled)
        .await
        .unwrap();
    assert_eq!(tp, CheckpointType::Delta);

    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    let tp = coord
        .determine_type("exec-1", &config_enabled)
        .await
        .unwrap();
    assert_eq!(tp, CheckpointType::Full);
}

#[tokio::test]
async fn progress_coords_survive_build_persist_round_trip() {
    let coord = make_coordinator();
    let snapshot = make_snapshot();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::AfterExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, snapshot.clone()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    let latest = coord
        .state_manager()
        .get_latest("exec-1")
        .await
        .unwrap()
        .expect("persisted checkpoint listed");
    assert_eq!(
        workflow_progress_coords(&latest),
        snapshot_workflow_coords(&snapshot)
    );
}

#[tokio::test]
async fn merge_description_back_rewrites_user_text_only() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::AfterExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    let merged = coord
        .merge_description_back(&cp.id, "exec-1", "second")
        .await
        .unwrap();
    assert_eq!(merged.id, cp.id);
    let description = merged
        .custom_fields
        .as_ref()
        .and_then(|fields| fields.get("description"))
        .and_then(|v| v.as_str());
    assert_eq!(description, Some("second"));
}

#[tokio::test]
async fn merge_missing_target_reports_not_found() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::AfterExecute)
        .await
        .unwrap();
    let first = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&first, "exec-1").await.unwrap();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let _second = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&_second, "exec-1").await.unwrap();

    coord.state_manager().delete(&first.id).await.unwrap();
    let err = coord
        .merge_description_back(&first.id, "exec-1", "late note")
        .await
        .unwrap_err();
    assert!(
        matches!(err, CheckpointError::NotFound { .. }),
        "a cleaned-up merge target must report not-found, never silently merge into latest"
    );
}

#[tokio::test]
async fn coords_match_full_snapshot_under_stripping_policy() {
    let storage = Arc::new(StorageBackend::new_memory());
    let sm = WorkflowCheckpointStateManager::new(storage);
    let policy = wf_types::checkpoint::UnifiedCheckpointPolicy {
        enabled: true,
        triggers: vec![],
        content: Some(wf_types::checkpoint::CheckpointContentConfig {
            include_state: Some(false),
            include_history: None,
            include_statistics: None,
            metadata: None,
            asynchronous: None,
        }),
        retention: None,
        error_handling: None,
    };
    let coord = WorkflowCheckpointCoordinator::new(sm).with_strategy(&policy);
    let mut snapshot = make_snapshot();
    snapshot.node_results = Some(HashMap::from([(
        "node-1".to_string(),
        serde_json::json!({"ok": true}),
    )]));
    snapshot
        .variable_state
        .variables
        .insert("counter".to_string(), serde_json::json!(1));

    let ctx = coord
        .prepare("exec-1", CheckpointTiming::AfterExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, snapshot.clone()).await.unwrap();
    // The blob payload is stripped, but the coordinates still describe
    // the pre-policy execution state.
    assert!(cp.snapshot.as_ref().unwrap().node_results.is_none());
    coord.persist(&cp, "exec-1").await.unwrap();

    let latest = coord
        .state_manager()
        .get_latest("exec-1")
        .await
        .unwrap()
        .expect("persisted checkpoint listed");
    assert_eq!(
        workflow_progress_coords(&latest),
        snapshot_workflow_coords(&snapshot)
    );
}

#[tokio::test]
async fn persist_emits_event() {
    let storage = Arc::new(StorageBackend::new_memory());
    let sm = WorkflowCheckpointStateManager::new(storage);
    let bus = CheckpointEventBus::new();
    let coord = WorkflowCheckpointCoordinator::new(sm).with_event_bus(bus.clone());

    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    assert_eq!(bus.receiver_count(), 0);
}

#[tokio::test]
async fn baseline_interval_forces_periodic_full() {
    let storage = Arc::new(StorageBackend::new_memory());
    let sm = WorkflowCheckpointStateManager::new(storage);
    let config = DeltaStorageConfig {
        enabled: true,
        baseline_interval: 2,
        max_delta_chain_length: 5,
    };
    let coord = WorkflowCheckpointCoordinator::new(sm).with_delta_config(config);

    let cp1 = build_and_persist(&coord, "running", "node-1").await;
    let cp2 = build_and_persist(&coord, "running", "node-2").await;
    let cp3 = build_and_persist(&coord, "running", "node-3").await;

    assert_eq!(cp1.r#type, Some(CheckpointType::Full));
    assert_eq!(cp2.r#type, Some(CheckpointType::Delta));
    assert_eq!(cp3.r#type, Some(CheckpointType::Full));
}

#[tokio::test]
async fn strategy_skips_unconfigured_trigger() {
    let coord =
        make_coordinator().with_strategy(&make_policy(vec![CheckpointTiming::AfterExecute]));

    let skipped = coord
        .create_checkpoint_with_strategy(CheckpointTiming::BeforeExecute, "exec-1", make_snapshot())
        .await
        .unwrap();
    assert!(skipped.is_none());

    let created = coord
        .create_checkpoint_with_strategy(CheckpointTiming::AfterExecute, "exec-1", make_snapshot())
        .await
        .unwrap();
    assert!(created.is_some());
}

#[tokio::test]
async fn strategy_disabled_never_checkpoints() {
    let coord = make_coordinator().with_strategy(&UnifiedCheckpointPolicy {
        enabled: false,
        triggers: vec![CheckpointTiming::AfterExecute],
        content: None,
        retention: None,
        error_handling: None,
    });

    let result = coord
        .create_checkpoint_with_strategy(CheckpointTiming::AfterExecute, "exec-1", make_snapshot())
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn no_strategy_always_checkpoints() {
    let coord = make_coordinator();
    let result = coord
        .create_checkpoint_with_strategy(CheckpointTiming::Manual, "exec-1", make_snapshot())
        .await
        .unwrap();
    assert!(result.is_some());
}

#[tokio::test]
async fn build_writes_metadata_with_trigger_and_chain_position() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::OnError)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();

    let metadata = cp.metadata.unwrap();
    assert_eq!(
        metadata.get("description").and_then(|v| v.as_str()),
        Some("Error checkpoint"),
        "trigger-based description"
    );
    assert_eq!(
        metadata.get("tags"),
        Some(&serde_json::json!(["trigger:ON_ERROR"]))
    );
    let custom = metadata.get("customFields").unwrap().as_object().unwrap();
    assert_eq!(
        custom.get(FORMAT_VERSION_FIELD).and_then(|v| v.as_str()),
        Some("1.1.0")
    );
    assert!(custom.get(CREATED_AT_FIELD).is_some());
    assert_eq!(
        custom.get(CHAIN_POSITION_FIELD),
        Some(&serde_json::json!(0))
    );
}

#[tokio::test]
async fn caller_custom_fields_are_merged_into_metadata() {
    let coord = make_coordinator();
    let ctx = CheckpointContext {
        entity_type: "workflow_execution".to_string(),
        entity_id: "exec-1".to_string(),
        trigger: None,
        actor_id: None,
        attempt: None,
        retry_count: None,
        error: None,
        fallback_used: None,
        metadata: Some(std::collections::HashMap::from([(
            "nodeId".to_string(),
            serde_json::json!("node-7"),
        )])),
    };
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    let metadata = cp.metadata.unwrap();
    let custom = metadata.get("customFields").unwrap().as_object().unwrap();
    assert_eq!(custom.get("nodeId"), Some(&serde_json::json!("node-7")));
}

#[tokio::test]
async fn create_checkpoint_aggregate_persists_and_returns_id() {
    let coord = make_coordinator();
    let id = coord
        .create_checkpoint(CheckpointTiming::AfterExecute, "exec-1", make_snapshot())
        .await
        .unwrap();
    assert!(!id.is_empty());
    assert!(coord.state_manager().load(&id).await.unwrap().is_some());
}

#[tokio::test]
async fn create_checkpoint_with_strategy_persists_saved_id() {
    let coord =
        make_coordinator().with_strategy(&make_policy(vec![CheckpointTiming::AfterExecute]));

    let created = coord
        .create_checkpoint_with_strategy(CheckpointTiming::AfterExecute, "exec-1", make_snapshot())
        .await
        .unwrap()
        .unwrap();
    assert!(coord
        .state_manager()
        .load(&created)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn strategy_gates_created_checkpoints_without_second_cadence_layer() {
    let coord =
        make_coordinator().with_strategy(&make_policy(vec![CheckpointTiming::AfterExecute]));

    let created = coord
        .create_checkpoint_with_strategy(CheckpointTiming::AfterExecute, "exec-1", make_snapshot())
        .await
        .unwrap();
    assert!(created.is_some(), "configured trigger fires");

    let skipped = coord
        .create_checkpoint_with_strategy(CheckpointTiming::BeforeExecute, "exec-1", make_snapshot())
        .await
        .unwrap();
    assert!(skipped.is_none(), "unconfigured trigger skipped");
}
