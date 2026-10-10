use super::support::*;
use crate::state::CheckpointStateManager;
use checkpoint_base::error::CheckpointError;
use serde_json::json;
use std::sync::Arc;
use wf_metrics::CheckpointMetricsCollector;
use wf_storage::domain::store::Store;

#[tokio::test]
async fn save_and_load() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);

    let cp = TestCheckpoint {
        id: "cp-1".to_string(),
        checkpoint_type: None,
        entity_id: "exec-1".to_string(),
        timestamp: 1000,
        data: "snapshot".to_string(),
    };

    mgr.save(&cp, "test", "exec-1").await.unwrap();
    let loaded = mgr.load("cp-1").await.unwrap();
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().data, "snapshot");
}

#[tokio::test]
async fn load_tampered_payload_fails_and_marks_corrupted() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage.clone());

    let cp = TestCheckpoint {
        id: "cp-tamper".to_string(),
        checkpoint_type: None,
        entity_id: "exec-1".to_string(),
        timestamp: 1000,
        data: "snapshot-with-enough-bytes".to_string(),
    };
    mgr.save(&cp, "test", "exec-1").await.unwrap();

    // Flip a byte in the middle of the persisted payload without
    // touching the stored hash (simulates on-disk corruption).
    assert!(
        storage.corrupt_payload("cp-tamper", 12, 0xFF).await,
        "payload must be tampered"
    );

    let err = mgr.load("cp-tamper").await.unwrap_err();
    assert!(
        matches!(err, CheckpointError::Corrupted { .. }),
        "tampered payload must surface as Corrupted, got {err:?}"
    );

    // The metadata record is marked corrupted so recovery/listing can
    // see the checkpoint is unusable.
    let listed = storage.list(None).await.unwrap();
    let (_, meta) = listed
        .iter()
        .find(|(id, _)| id == "cp-tamper")
        .expect("tampered checkpoint metadata still listed");
    assert_eq!(
        meta.get("status").and_then(|v| v.as_str()),
        Some("corrupted"),
        "metadata status must be marked corrupted"
    );

    // Corrupted records are excluded from normal checkpoint loads.
    assert!(mgr.load("cp-tamper").await.is_err());
}

#[tokio::test]
async fn load_missing() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);
    let loaded = mgr.load("nonexistent").await.unwrap();
    assert!(loaded.is_none());
}

#[tokio::test]
async fn delete_existing() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);

    let cp = TestCheckpoint {
        id: "cp-1".to_string(),
        checkpoint_type: None,
        entity_id: "exec-1".to_string(),
        timestamp: 1000,
        data: "x".to_string(),
    };

    mgr.save(&cp, "test", "exec-1").await.unwrap();
    assert!(mgr.delete("cp-1").await.unwrap());
    assert!(!mgr.delete("cp-1").await.unwrap());
}

#[tokio::test]
async fn load_batch_reports_missing() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    mgr.save(
        &make_envelope("cp-1", None, None, 1000, None, Some(json!({"state": "a"}))),
        "test",
        "exec-1",
    )
    .await
    .unwrap();

    let loaded = mgr
        .load_batch(&["cp-1".to_string(), "missing".to_string()])
        .await
        .unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].as_ref().expect("cp-1 present").id, "cp-1");
    assert!(loaded[1].is_none());
}

#[tokio::test]
async fn metrics_recorded_on_save_and_load() {
    let storage = make_storage();
    let metrics = Arc::new(CheckpointMetricsCollector::new(
        wf_metrics::CollectorConfig::default(),
    ));
    let mgr = StorageBackedStateManager::<Envelope>::new(storage).with_metrics(metrics.clone());

    mgr.save(
        &make_envelope("cp-1", None, None, 1000, None, Some(json!({"state": "a"}))),
        "test",
        "exec-1",
    )
    .await
    .unwrap();

    let _ = mgr.load("cp-1").await.unwrap();
    let _ = mgr.load("missing").await.unwrap();

    let stats = metrics.usage_stats();
    assert_eq!(stats.creation_count, 1);
    assert_eq!(stats.load_count, 2);
    assert_eq!(stats.load_failures, 1);
}
