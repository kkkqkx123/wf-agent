use super::super::*;
use super::fixtures::*;
use wf_storage::backend::StorageBackend;

#[tokio::test]
async fn restore_from_full_checkpoint() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    let id = cp.id.clone();
    coord.persist(&cp, "loop-1").await.unwrap();

    let entity = coord.restore(&id).await.unwrap();
    assert_eq!(entity.agent_loop_id, "loop-1");
    assert_eq!(entity.status, "running");
}

#[tokio::test]
async fn recovery_reuses_same_actor_identity_as_workflow_path() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    assert_eq!(ctx.entity_type, "agent_loop");
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "loop-1").await.unwrap();
    let entity = coord.restore(&cp.id).await.unwrap();
    assert_eq!(entity.agent_loop_id, "loop-1");
    let latest = coord
        .state_manager()
        .get_latest("loop-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.entity_id, "loop-1");
}

#[tokio::test]
async fn delta_chain_restore_after_multiple_deltas() {
    let coord = make_coordinator();
    build_and_persist(&coord, "running", 1).await;
    build_and_persist(&coord, "running", 2).await;
    let cp3 = build_and_persist(&coord, "completed", 3).await;

    assert_eq!(cp3.r#type, Some(CheckpointType::Delta));

    let entity = coord.restore(&cp3.id).await.unwrap();
    assert_eq!(entity.status, "completed");
    assert_eq!(entity.current_iteration, 3);
}

#[tokio::test]
async fn delta_chain_base_points_to_snapshot_checkpoint() {
    let coord = make_coordinator();
    let cp1 = build_and_persist(&coord, "running", 1).await;
    build_and_persist(&coord, "running", 2).await;
    let cp3 = build_and_persist(&coord, "completed", 3).await;

    assert_eq!(cp1.r#type, Some(CheckpointType::Full));
    assert_eq!(cp3.r#type, Some(CheckpointType::Delta));
    assert_eq!(cp3.base_checkpoint_id.as_deref(), Some(cp1.id.as_str()));
}

#[tokio::test]
async fn fallback_to_full_when_chain_base_missing() {
    let coord = make_coordinator();
    let cp1 = build_and_persist(&coord, "running", 1).await;
    let cp2 = build_and_persist(&coord, "running", 2).await;
    assert_eq!(cp2.r#type, Some(CheckpointType::Delta));

    coord.state_manager().delete(&cp1.id).await.unwrap();

    let cp3 = build_and_persist(&coord, "running", 3).await;
    assert_eq!(cp3.r#type, Some(CheckpointType::Full));
    assert!(cp3.snapshot.is_some());

    let entity = coord.restore(&cp3.id).await.unwrap();
    assert_eq!(entity.current_iteration, 3);
}

#[tokio::test]
async fn restore_migrates_old_format_version() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let mut cp = coord.build(ctx, make_snapshot()).await.unwrap();
    cp.format_version = Some("1.0.0".to_string());
    coord.persist(&cp, "loop-1").await.unwrap();

    let entity = coord.restore(&cp.id).await.unwrap();
    assert_eq!(entity.agent_loop_id, "loop-1");
}

#[tokio::test]
async fn restore_rejects_incompatible_version() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let mut cp = coord.build(ctx, make_snapshot()).await.unwrap();
    cp.format_version = Some("0.5.0".to_string());
    coord.persist(&cp, "loop-1").await.unwrap();

    let err = coord.restore(&cp.id).await.unwrap_err();
    assert!(matches!(err, CheckpointError::VersionIncompatible { .. }));
}

#[tokio::test]
async fn restore_restores_file_checkpoint() {
    use checkpoint_file::file::{FileCheckpointManager, FileContentEntry};

    let storage = Arc::new(StorageBackend::new_memory());
    let sm = AgentCheckpointStateManager::new(storage);

    let file_manager = FileCheckpointManager::new_in_memory().unwrap();
    let file_manager2 = file_manager.clone();
    file_manager
        .create_checkpoint(
            "loop-1",
            &[FileContentEntry::new("a.txt", b"hello".to_vec())],
        )
        .unwrap();

    let coord = AgentCheckpointCoordinator::new(sm).with_file_checkpoint_manager(file_manager2);

    let ctx = coord
        .prepare("loop-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "loop-1").await.unwrap();

    let entity = coord.restore(&cp.id).await.unwrap();
    assert_eq!(entity.agent_loop_id, "loop-1");
    let workspace = file_manager.get_actor_workspace("agent:loop-1").unwrap();
    assert!(
        workspace.iter().any(|f| f.path == "a.txt"),
        "actor edit line stored via coordinator query view"
    );
}

#[tokio::test]
async fn timeline_prefers_metadata_without_blob_regression() {
    let coord = make_coordinator();
    let mut snapshot = make_snapshot();
    snapshot.message_seq_start = Some(7);
    snapshot.message_seq_end = Some(9);
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::Manual)
        .await
        .unwrap();
    let cp = coord.build(ctx, snapshot).await.unwrap();
    let cp_id = cp.id.clone();
    coord.persist(&cp, "loop-1").await.unwrap();

    let rows = coord.timeline("loop-1").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, cp_id);
    assert_eq!(rows[0].1, Some(7));
    assert_eq!(rows[0].2, Some(9));
    assert_eq!(rows[0].3.as_deref(), Some("Manual checkpoint"));
}
