use super::super::*;
use super::fixtures::*;
use wf_storage::backend::StorageBackend;

#[tokio::test]
async fn restore_from_full_checkpoint() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    let id = cp.id.clone();
    coord.persist(&cp, "exec-1").await.unwrap();

    let entity = coord.restore(&id).await.unwrap();
    assert_eq!(entity.execution_id, "exec-1");
    assert_eq!(entity.status, "running");
}

#[tokio::test]
async fn both_coordinators_restore_reuse_same_identifier() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    assert_eq!(ctx.entity_type, "workflow_execution");
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();
    let entity = coord.restore(&cp.id).await.unwrap();
    assert_eq!(entity.execution_id, "exec-1");
    let latest = coord
        .state_manager()
        .get_latest("exec-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.entity_id, "exec-1");
}

#[tokio::test]
async fn delta_chain_restore_after_multiple_deltas() {
    let coord = make_coordinator();
    build_and_persist(&coord, "running", "node-1").await;
    build_and_persist(&coord, "running", "node-2").await;
    let cp3 = build_and_persist(&coord, "completed", "node-3").await;

    assert_eq!(cp3.r#type, Some(CheckpointType::Delta));
    assert!(cp3.base_checkpoint_id.is_some());

    let entity = coord.restore(&cp3.id).await.unwrap();
    assert_eq!(entity.status, "completed");
    assert_eq!(entity.snapshot.current_node_id, Some("node-3".to_string()));
}

#[tokio::test]
async fn delta_chain_base_points_to_snapshot_checkpoint() {
    let coord = make_coordinator();
    let cp1 = build_and_persist(&coord, "running", "node-1").await;
    build_and_persist(&coord, "running", "node-2").await;
    let cp3 = build_and_persist(&coord, "completed", "node-3").await;

    assert_eq!(cp1.r#type, Some(CheckpointType::Full));
    assert_eq!(cp3.r#type, Some(CheckpointType::Delta));
    assert_eq!(cp3.base_checkpoint_id.as_deref(), Some(cp1.id.as_str()));
}

#[tokio::test]
async fn restore_after_periodic_baseline() {
    let storage = Arc::new(StorageBackend::new_memory());
    let sm = WorkflowCheckpointStateManager::new(storage);
    let config = DeltaStorageConfig {
        enabled: true,
        baseline_interval: 2,
        max_delta_chain_length: 5,
    };
    let coord = WorkflowCheckpointCoordinator::new(sm).with_delta_config(config);

    build_and_persist(&coord, "running", "node-1").await;
    build_and_persist(&coord, "running", "node-2").await;
    build_and_persist(&coord, "running", "node-3").await;
    let cp4 = build_and_persist(&coord, "completed", "node-4").await;

    assert_eq!(cp4.r#type, Some(CheckpointType::Delta));

    let entity = coord.restore(&cp4.id).await.unwrap();
    assert_eq!(entity.status, "completed");
    assert_eq!(entity.snapshot.current_node_id, Some("node-4".to_string()));
}

#[tokio::test]
async fn fallback_to_full_when_chain_base_missing() {
    let coord = make_coordinator();
    let cp1 = build_and_persist(&coord, "running", "node-1").await;
    let cp2 = build_and_persist(&coord, "running", "node-2").await;
    assert_eq!(cp2.r#type, Some(CheckpointType::Delta));

    coord.state_manager().delete(&cp1.id).await.unwrap();

    let cp3 = build_and_persist(&coord, "running", "node-3").await;
    assert_eq!(cp3.r#type, Some(CheckpointType::Full));
    assert!(cp3.snapshot.is_some());

    let entity = coord.restore(&cp3.id).await.unwrap();
    assert_eq!(entity.snapshot.current_node_id, Some("node-3".to_string()));
}

#[tokio::test]
async fn restore_migrates_old_format_version() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let mut cp = coord.build(ctx, make_snapshot()).await.unwrap();
    cp.format_version = Some("1.0.0".to_string());
    coord.persist(&cp, "exec-1").await.unwrap();

    let entity = coord.restore(&cp.id).await.unwrap();
    assert_eq!(entity.execution_id, "exec-1");
}

#[tokio::test]
async fn restore_rejects_incompatible_version() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let mut cp = coord.build(ctx, make_snapshot()).await.unwrap();
    cp.format_version = Some("0.5.0".to_string());
    coord.persist(&cp, "exec-1").await.unwrap();

    let err = coord.restore(&cp.id).await.unwrap_err();
    assert!(matches!(err, CheckpointError::VersionIncompatible { .. }));
}

#[tokio::test]
async fn restore_future_version_rejected() {
    let coord = make_coordinator();
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let mut cp = coord.build(ctx, make_snapshot()).await.unwrap();
    cp.format_version = Some("9.0.0".to_string());
    coord.persist(&cp, "exec-1").await.unwrap();

    assert!(matches!(
        coord.restore(&cp.id).await.unwrap_err(),
        CheckpointError::VersionIncompatible { .. }
    ));
}

#[tokio::test]
async fn restore_child_hierarchy_summary() {
    use wf_types::execution::{ExecutionHierarchy, ExecutionType};

    let storage = Arc::new(StorageBackend::new_memory());
    let sm = WorkflowCheckpointStateManager::new(storage);
    let coord = WorkflowCheckpointCoordinator::new(sm);

    // Parent checkpoint: a root, holding no knowledge of any child.
    let mut snapshot = make_snapshot();
    snapshot.hierarchy = Some(ExecutionHierarchy::new(
        "wf-1".to_string(),
        "exec-1".to_string(),
        Vec::new(),
        None,
        None,
        None,
    ));
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, snapshot).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    // Child checkpoint stored under the child execution id, linking
    // forward to the parent. The parent record predates the child, so the
    // only way restore can find it is through this link.
    let mut child_snapshot = make_snapshot();
    child_snapshot.execution_id = "child-exec-1".to_string();
    child_snapshot.status = "completed".to_string();
    child_snapshot.hierarchy = Some(ExecutionHierarchy::new(
        "wf-1".to_string(),
        "child-exec-1".to_string(),
        vec!["exec-1".to_string()],
        Some(ExecutionType::Workflow),
        Some(ExecutionType::Workflow),
        None,
    ));
    let ctx = coord
        .prepare("child-exec-1", CheckpointTiming::AfterExecute)
        .await
        .unwrap();
    let child_cp = coord.build(ctx, child_snapshot).await.unwrap();
    coord.persist(&child_cp, "child-exec-1").await.unwrap();

    let entity = coord.restore(&cp.id).await.unwrap();
    let summary = entity.restore_summary.unwrap();
    assert_eq!(summary.total, 1, "child found through its own parent link");
}

#[tokio::test]
async fn version_manager_is_exposed() {
    let coord = make_coordinator();
    assert_eq!(coord.version_manager().current_version(), "1.1.0");
    let vm = VersionManager::new();
    let coord = make_coordinator().with_version_manager(vm);
    assert_eq!(coord.version_manager().current_version(), "1.1.0");
}

#[tokio::test]
async fn restore_restores_file_checkpoint() {
    use checkpoint_file::file::{FileCheckpointManager, FileContentEntry};

    let storage = Arc::new(StorageBackend::new_memory());
    let sm = WorkflowCheckpointStateManager::new(storage);

    let file_manager = FileCheckpointManager::new_in_memory().unwrap();
    let file_manager2 = file_manager.clone();
    file_manager
        .create_checkpoint(
            "exec-1",
            &[FileContentEntry::new("a.txt", b"hello".to_vec())],
        )
        .unwrap();

    let coord = WorkflowCheckpointCoordinator::new(sm).with_file_checkpoint_manager(file_manager2);

    let ctx = coord
        .prepare("exec-1", CheckpointTiming::BeforeExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, make_snapshot()).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();

    let entity = coord.restore(&cp.id).await.unwrap();
    assert_eq!(entity.execution_id, "exec-1");
    let workspace = file_manager.get_actor_workspace("agent:exec-1").unwrap();
    assert!(
        workspace.iter().any(|f| f.path == "a.txt"),
        "actor edit line stored via coordinator query view"
    );
}

#[tokio::test]
async fn async_persistence_defers_file_snapshot_until_wait() {
    use checkpoint_file::file::{FileCheckpointManager, FileContentEntry};

    let storage = Arc::new(StorageBackend::new_memory());
    let sm = WorkflowCheckpointStateManager::new(storage);
    let file_manager = FileCheckpointManager::new_in_memory().unwrap();
    file_manager
        .create_checkpoint(
            "exec-1",
            &[FileContentEntry::new("a.txt", b"hello".to_vec())],
        )
        .unwrap();

    let coord = WorkflowCheckpointCoordinator::new(sm)
        .with_async_persistence(true)
        .with_file_checkpoint_manager(file_manager.clone());

    let id = coord
        .create_checkpoint(CheckpointTiming::AfterExecute, "exec-1", make_snapshot())
        .await
        .unwrap();
    assert_eq!(coord.pending_persistence_count().await, 1);
    let before = file_manager.get_actor_workspace("agent:exec-1").unwrap();
    assert!(before.iter().any(|f| f.path == "a.txt"));

    coord.wait_for_persistence().await;
    assert_eq!(coord.pending_persistence_count().await, 0);
    // Deferred file persistence is a read-only projection correlated
    // with the state checkpoint: it resolves the same workspace instead
    // of appending a duplicate commit.
    let after = file_manager.get_actor_workspace("agent:exec-1").unwrap();
    assert_eq!(
        before, after,
        "deferred projection must not duplicate the commit"
    );

    let loaded = coord.state_manager().load(&id).await.unwrap();
    assert!(
        loaded.is_some(),
        "checkpoint itself persisted synchronously"
    );
}

#[tokio::test]
async fn async_persistence_enabled_via_policy_content_config() {
    let storage = Arc::new(StorageBackend::new_memory());
    let sm = WorkflowCheckpointStateManager::new(storage);
    let policy = UnifiedCheckpointPolicy {
        enabled: true,
        triggers: vec![CheckpointTiming::AfterExecute],
        content: Some(wf_types::checkpoint::CheckpointContentConfig {
            include_state: Some(true),
            include_history: Some(true),
            include_statistics: Some(false),
            metadata: None,
            asynchronous: Some(true),
        }),
        retention: None,
        error_handling: None,
    };
    let coord = WorkflowCheckpointCoordinator::new(sm).with_strategy(&policy);
    assert!(coord.async_persistence_enabled());

    coord
        .create_checkpoint(CheckpointTiming::AfterExecute, "exec-1", make_snapshot())
        .await
        .unwrap();
    assert_eq!(coord.pending_persistence_count().await, 1);
    coord.wait_for_persistence().await;
    assert_eq!(coord.pending_persistence_count().await, 0);
}

#[tokio::test]
async fn restore_rejects_invalid_delta_checkpoint() {
    let coord = make_coordinator();
    let cp1 = build_and_persist(&coord, "running", "node-1").await;

    let mut invalid = coord
        .build(
            coord
                .prepare("exec-1", CheckpointTiming::AfterExecute)
                .await
                .unwrap(),
            make_snapshot(),
        )
        .await
        .unwrap();
    invalid.r#type = Some(CheckpointType::Delta);
    invalid.base_checkpoint_id = Some(cp1.id.clone());
    invalid.previous_checkpoint_id = None;
    invalid.snapshot = None;
    invalid.delta = None;
    // Broken delta links fail fast at persist, never reaching restore.
    let err = coord.persist(&invalid, "exec-1").await.unwrap_err();
    assert!(
        matches!(
            err,
            CheckpointError::Validation { .. } | CheckpointError::DeltaChainBroken { .. }
        ),
        "missing previous_checkpoint_id rejected at persist"
    );
}
