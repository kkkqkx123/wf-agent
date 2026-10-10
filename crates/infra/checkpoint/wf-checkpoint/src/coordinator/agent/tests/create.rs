use super::super::*;
use super::fixtures::*;
use checkpoint_base::metadata::builder::{
    CHAIN_POSITION_FIELD, CREATED_AT_FIELD, FORMAT_VERSION_FIELD,
};
use wf_storage::backend::StorageBackend;

#[tokio::test]
async fn prepare_returns_context() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    assert_eq!(ctx.entity_type, "agent_loop");
    assert_eq!(ctx.entity_id, "loop-1");
}

#[tokio::test]
async fn build_creates_full_checkpoint_on_first_save() {
    let coord = make_coordinator();
    let ctx = CheckpointContext {
        entity_type: "agent_loop".to_string(),
        entity_id: "loop-1".to_string(),
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
        .prepare("loop-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "loop-1").await.unwrap();

    let loaded = coord.state_manager().load(&cp.id).await.unwrap();
    assert!(loaded.is_some());
}

#[tokio::test]
async fn build_delta_after_full() {
    let coord = make_coordinator();

    let ctx1 = coord
        .prepare("loop-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp1 = coord.build(ctx1, make_snapshot()).await.unwrap();
    coord.persist(&cp1, "loop-1").await.unwrap();

    let mut snapshot2 = make_snapshot();
    snapshot2.current_iteration = 2;
    let ctx2 = coord
        .prepare("loop-1", CheckpointTiming::AfterExecute)
        .await
        .unwrap();
    let cp2 = coord.build(ctx2, snapshot2).await.unwrap();
    assert_eq!(cp2.r#type, Some(CheckpointType::Delta));
    assert!(cp2.delta.is_some());
}

#[tokio::test]
async fn progress_coords_survive_build_persist_round_trip() {
    let coord = make_coordinator();
    let snapshot = make_snapshot();
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::Manual)
        .await
        .unwrap();
    let cp = coord.build(ctx, snapshot.clone()).await.unwrap();
    coord.persist(&cp, "loop-1").await.unwrap();

    let latest = coord
        .state_manager()
        .get_latest("loop-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        progress_coords(&latest),
        snapshot_progress_coords(&snapshot)
    );
}

#[tokio::test]
async fn pending_count_participates_in_coords() {
    let coord = make_coordinator();
    let mut snapshot = make_snapshot();
    snapshot.pending_tool_call_ids = Some(vec!["call-1".to_string()]);
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::Manual)
        .await
        .unwrap();
    let cp = coord.build(ctx, snapshot.clone()).await.unwrap();
    coord.persist(&cp, "loop-1").await.unwrap();

    let latest = coord
        .state_manager()
        .get_latest("loop-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        progress_coords(&latest),
        snapshot_progress_coords(&snapshot)
    );
    let idle = make_snapshot();
    assert_ne!(progress_coords(&latest), snapshot_progress_coords(&idle));
}

#[tokio::test]
async fn merge_description_back_rewrites_user_text_only() {
    let coord = make_coordinator();
    let mut ctx = coord
        .prepare("loop-1", CheckpointTiming::Manual)
        .await
        .unwrap();
    ctx.metadata
        .get_or_insert_default()
        .insert("description".to_string(), serde_json::json!("first"));
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "loop-1").await.unwrap();

    let merged = coord
        .merge_description_back(&cp.id, "loop-1", "second")
        .await
        .unwrap();
    assert_eq!(merged.id, cp.id);
    let stored = merged
        .custom_fields
        .as_ref()
        .and_then(|fields| fields.get("description"))
        .and_then(|v| v.as_str());
    assert_eq!(stored, Some("second"));

    // The trigger label is untouched by the merge.
    let loaded = coord.state_manager().load(&cp.id).await.unwrap().unwrap();
    let trigger_label = loaded
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("description"))
        .and_then(|v| v.as_str());
    assert_eq!(trigger_label, Some("Manual checkpoint"));

    // Still a single row: the merge never allocates a checkpoint.
    assert_eq!(
        coord
            .state_manager()
            .count_by_entity("loop-1")
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn merge_missing_target_reports_not_found() {
    let coord = make_coordinator();
    let first = build_and_persist(&coord, "running", 1).await;
    let mut second_snapshot = make_snapshot();
    second_snapshot.current_iteration = 2;
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::AfterExecute)
        .await
        .unwrap();
    let _second = coord.build(ctx, second_snapshot).await.unwrap();
    coord.persist(&_second, "loop-1").await.unwrap();

    coord.state_manager().delete(&first.id).await.unwrap();
    let err = coord
        .merge_description_back(&first.id, "loop-1", "late note")
        .await
        .unwrap_err();
    assert!(
        matches!(err, CheckpointError::NotFound { .. }),
        "a cleaned-up merge target must report not-found, never silently merge into latest"
    );
}

#[tokio::test]
async fn baseline_interval_forces_periodic_full() {
    let storage = Arc::new(StorageBackend::new_memory());
    let sm = AgentCheckpointStateManager::new(storage);
    let config = DeltaStorageConfig {
        enabled: true,
        baseline_interval: 2,
        max_delta_chain_length: 5,
    };
    let coord = AgentCheckpointCoordinator::new(sm).with_delta_config(config);

    let cp1 = build_and_persist(&coord, "running", 1).await;
    let cp2 = build_and_persist(&coord, "running", 2).await;
    let cp3 = build_and_persist(&coord, "running", 3).await;

    assert_eq!(cp1.r#type, Some(CheckpointType::Full));
    assert_eq!(cp2.r#type, Some(CheckpointType::Delta));
    assert_eq!(cp3.r#type, Some(CheckpointType::Full));
}

#[tokio::test]
async fn strategy_skips_unconfigured_trigger() {
    let coord =
        make_coordinator().with_strategy(&make_policy(vec![CheckpointTiming::AfterExecute]));

    let skipped = coord
        .create_checkpoint_with_strategy(CheckpointTiming::BeforeExecute, "loop-1", make_snapshot())
        .await
        .unwrap();
    assert!(skipped.is_none());

    let created = coord
        .create_checkpoint_with_strategy(CheckpointTiming::AfterExecute, "loop-1", make_snapshot())
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
        .create_checkpoint_with_strategy(CheckpointTiming::AfterExecute, "loop-1", make_snapshot())
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn build_writes_metadata_with_trigger_and_chain_position() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::AfterExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();

    let metadata = cp.metadata.unwrap();
    assert_eq!(
        metadata.get("description").and_then(|v| v.as_str()),
        Some("After execute"),
        "trigger-based description"
    );
    assert_eq!(
        metadata.get("tags"),
        Some(&serde_json::json!(["trigger:AFTER_EXECUTE"]))
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
async fn delta_metadata_carries_incremented_chain_position() {
    let coord = make_coordinator();
    build_and_persist(&coord, "running", 1).await;
    let cp2 = build_and_persist(&coord, "running", 2).await;

    assert_eq!(cp2.r#type, Some(CheckpointType::Delta));
    let metadata = cp2.metadata.unwrap();
    let custom = metadata.get("customFields").unwrap().as_object().unwrap();
    assert_eq!(
        custom.get(CHAIN_POSITION_FIELD),
        Some(&serde_json::json!(1)),
        "delta chain position inherited from previous checkpoint"
    );
}

#[tokio::test]
async fn create_checkpoint_aggregate_persists_and_returns_id() {
    let coord = make_coordinator();
    let id = coord
        .create_checkpoint(CheckpointTiming::AfterExecute, "loop-1", make_snapshot())
        .await
        .unwrap();
    assert!(!id.is_empty());

    let loaded = coord.state_manager().load(&id).await.unwrap();
    assert!(loaded.is_some(), "aggregate create persists the checkpoint");
}

#[tokio::test]
async fn create_checkpoint_returns_saved_id_for_matching_trigger() {
    let coord =
        make_coordinator().with_strategy(&make_policy(vec![CheckpointTiming::AfterExecute]));

    let skipped = coord
        .create_checkpoint_with_strategy(CheckpointTiming::BeforeExecute, "loop-1", make_snapshot())
        .await
        .unwrap();
    assert!(skipped.is_none());

    let created = coord
        .create_checkpoint_with_strategy(CheckpointTiming::AfterExecute, "loop-1", make_snapshot())
        .await
        .unwrap();
    assert!(created.is_some(), "persisted id returned");
    let id = created.unwrap();
    assert!(coord.state_manager().load(&id).await.unwrap().is_some());
}

#[tokio::test]
async fn restore_rejects_invalid_delta_checkpoint() {
    let coord = make_coordinator();
    let cp1 = build_and_persist(&coord, "running", 1).await;

    let mut invalid = coord
        .build(
            coord
                .prepare("loop-1", CheckpointTiming::AfterExecute)
                .await
                .unwrap(),
            make_snapshot(),
        )
        .await
        .unwrap();
    invalid.r#type = Some(CheckpointType::Delta);
    invalid.base_checkpoint_id = None;
    invalid.previous_checkpoint_id = Some(cp1.id.clone());
    invalid.snapshot = None;
    invalid.delta = None;
    coord.persist(&invalid, "loop-1").await.unwrap();

    let err = coord.restore(&invalid.id).await.unwrap_err();
    assert!(
        matches!(err, CheckpointError::Validation { .. }),
        "missing delta fields rejected before restore"
    );
}
