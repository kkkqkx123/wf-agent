//! Unit tests for the storage-backed state manager.

use super::metadata::parse_storage_metadata;
use super::StorageBackedStateManager;
use crate::state::CheckpointStateManager;
use checkpoint_base::cleanup_policy::CleanupStrategy;
use checkpoint_base::delta::{CheckpointLoader, DiffCalculator};
use checkpoint_base::error::CheckpointError;
use serde_json::{json, Value};
use std::sync::Arc;
use wf_metrics::CheckpointMetricsCollector;
use wf_storage::backend::StorageBackend;
use wf_storage::domain::store::Store;
use wf_types::checkpoint::{BaseCheckpointCore, CheckpointType};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
struct TestCheckpoint {
    id: String,
    checkpoint_type: Option<String>,
    entity_id: String,
    timestamp: i64,
    data: String,
}

fn make_storage() -> Arc<StorageBackend> {
    Arc::new(StorageBackend::new_memory())
}

type Envelope = BaseCheckpointCore<Value, Value>;

fn make_envelope(
    id: &str,
    cp_type: Option<CheckpointType>,
    previous: Option<&str>,
    timestamp: i64,
    delta: Option<Value>,
    snapshot: Option<Value>,
) -> Envelope {
    BaseCheckpointCore {
        id: id.to_string(),
        r#type: cp_type,
        base_checkpoint_id: previous.map(String::from),
        previous_checkpoint_id: previous.map(String::from),
        delta,
        snapshot,
        timestamp: Some(timestamp),
        metadata: None,
        format_version: None,
    }
}

/// Trivial diff calculator where the delta carries the entire current
/// state: diff(prev, curr) = curr, apply(base, delta) = delta.
struct FullStateDiff;

#[async_trait::async_trait]
impl DiffCalculator<Value, Value> for FullStateDiff {
    async fn calculate_diff(
        &self,
        _previous: &Value,
        current: &Value,
    ) -> Result<Value, CheckpointError> {
        Ok(current.clone())
    }

    async fn apply_delta(&self, _base: &Value, delta: &Value) -> Result<Value, CheckpointError> {
        Ok(delta.clone())
    }
}

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
async fn list_by_entity_filters_correctly() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);

    let cp1 = TestCheckpoint {
        id: "cp-1".to_string(),
        checkpoint_type: None,
        entity_id: "exec-1".to_string(),
        timestamp: 1000,
        data: "x".to_string(),
    };
    let cp2 = TestCheckpoint {
        id: "cp-2".to_string(),
        checkpoint_type: None,
        entity_id: "exec-2".to_string(),
        timestamp: 2000,
        data: "y".to_string(),
    };

    mgr.save(&cp1, "test", "exec-1").await.unwrap();
    mgr.save(&cp2, "test", "exec-2").await.unwrap();

    let list = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "cp-1");
}

#[tokio::test]
async fn cleanup_removes_oldest() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);

    for i in 0..5 {
        let cp = TestCheckpoint {
            id: format!("cp-{}", i),
            checkpoint_type: None,
            entity_id: "exec-1".to_string(),
            timestamp: i as i64 * 1000,
            data: format!("data-{}", i),
        };
        mgr.save(&cp, "test", "exec-1").await.unwrap();
    }

    let deleted = mgr.cleanup("exec-1", Some(2)).await.unwrap();
    assert_eq!(deleted, 3);

    let remaining = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(remaining.len(), 2);
}

#[tokio::test]
async fn cleanup_protects_delta_chain_members() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    mgr.save(
        &make_envelope(
            "full-1",
            None,
            None,
            1000,
            None,
            Some(json!({"state": "base"})),
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();
    mgr.save(
        &make_envelope(
            "delta-1",
            Some(CheckpointType::Delta),
            Some("full-1"),
            2000,
            Some(json!({"state": "s1"})),
            None,
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();
    mgr.save(
        &make_envelope(
            "delta-2",
            Some(CheckpointType::Delta),
            Some("delta-1"),
            3000,
            Some(json!({"state": "s2"})),
            None,
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();
    mgr.save(
        &make_envelope(
            "delta-3",
            Some(CheckpointType::Delta),
            Some("delta-2"),
            4000,
            Some(json!({"state": "s3"})),
            None,
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();

    let deleted = mgr.cleanup("exec-1", Some(2)).await.unwrap();
    assert_eq!(deleted, 0);

    let remaining = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(remaining.len(), 4);
}

#[tokio::test]
async fn execute_cleanup_for_entity_respects_exclude_and_reports_bytes() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    for i in 0..4 {
        mgr.save(
            &make_envelope(
                &format!("cp-{}", i),
                None,
                None,
                1000 + i as i64,
                None,
                Some(json!({"state": i})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
    }

    let result = mgr
        .execute_cleanup_for_entity(
            "exec-1",
            "workflow_execution",
            Some("cp-0"),
            &CleanupStrategy::CountBased {
                max_checkpoints: 1,
                min_retention: 0,
            },
        )
        .await
        .unwrap();

    assert_eq!(result.deleted_count, 2);
    assert!(
        !result.deleted_checkpoint_ids.contains(&"cp-0".to_string()),
        "excluded checkpoint survives cleanup"
    );
    assert!(result.deleted_checkpoint_ids.contains(&"cp-1".to_string()));
    assert!(result.deleted_checkpoint_ids.contains(&"cp-2".to_string()));
    assert_eq!(result.remaining_count, 2);
    assert!(
        result.freed_bytes > 0,
        "freed bytes accounted from real blob sizes"
    );
}

#[tokio::test]
async fn cleanup_uses_watermark_for_incremental_runs() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);
    let strategy = CleanupStrategy::CountBased {
        max_checkpoints: 1,
        min_retention: 0,
    };

    for i in 0..5 {
        mgr.save(
            &make_envelope(
                &format!("cp-{}", i),
                None,
                None,
                1000 + i as i64,
                None,
                Some(json!({"state": i})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
    }

    // First run is a full scan: only the newest checkpoint survives.
    let r1 = mgr
        .execute_cleanup_for_entity("exec-1", "test", None, &strategy)
        .await
        .unwrap();
    assert_eq!(r1.deleted_count, 4);
    let remaining = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "cp-4");

    // Newer checkpoints arrive after the watermark was persisted.
    for i in 5..7 {
        mgr.save(
            &make_envelope(
                &format!("cp-{}", i),
                None,
                None,
                5000 + (i - 5) as i64,
                None,
                Some(json!({"state": i})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
    }

    // Second run is incremental: only checkpoints newer than the
    // watermark are considered, so the old survivor is untouched.
    let r2 = mgr
        .execute_cleanup_for_entity("exec-1", "test", None, &strategy)
        .await
        .unwrap();
    assert_eq!(r2.deleted_count, 1);
    let remaining = mgr.list_by_entity("exec-1").await.unwrap();
    let ids: Vec<&str> = remaining.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["cp-4", "cp-6"]);

    let (watermark, run_count) = mgr.load_entity_cleanup_metadata("exec-1").await.unwrap();
    assert_eq!(watermark, Some(5001));
    assert_eq!(run_count, 2);
}

#[tokio::test]
async fn cleanup_watermark_clamps_future_and_advances_on_empty() {
    use checkpoint_base::clock::CheckpointClock;

    const T0: i64 = 1_000_000;
    let storage = make_storage();
    let mgr =
        StorageBackedStateManager::<Envelope>::new(storage).with_clock(CheckpointClock::manual(T0));
    let strategy = CleanupStrategy::CountBased {
        max_checkpoints: 10,
        min_retention: 0,
    };

    let future = T0 + 86_400_000;
    mgr.save(
        &make_envelope(
            "cp-future",
            None,
            None,
            future,
            None,
            Some(json!({"state": "future"})),
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();

    let result = mgr
        .execute_cleanup_for_entity("exec-1", "test", None, &strategy)
        .await
        .unwrap();
    assert_eq!(result.deleted_count, 0);
    let (watermark, run_count) = mgr.load_entity_cleanup_metadata("exec-1").await.unwrap();
    // Deterministic clamp: the future survivor cannot lift the watermark
    // past the clock reading.
    assert_eq!(watermark, Some(T0));
    assert_eq!(run_count, 1);

    let second = mgr
        .execute_cleanup_for_entity("exec-1", "test", None, &strategy)
        .await
        .unwrap();
    assert_eq!(second.deleted_count, 0);
    let (_, run_count) = mgr.load_entity_cleanup_metadata("exec-1").await.unwrap();
    assert_eq!(run_count, 2);
}

#[tokio::test]
async fn cleanup_without_clock_refuses_watermark_write() {
    use checkpoint_base::clock::{CheckpointClock, ManualClock};

    let storage = make_storage();
    let clock = CheckpointClock::manual(1_000_000);
    let handle: ManualClock = clock.manual_handle().expect("manual clock");
    let mgr = StorageBackedStateManager::<Envelope>::new(storage).with_clock(clock);
    let strategy = CleanupStrategy::CountBased {
        max_checkpoints: 10,
        min_retention: 0,
    };
    mgr.save(
        &make_envelope("cp-1", None, None, 1000, None, Some(json!({"state": 1}))),
        "test",
        "exec-1",
    )
    .await
    .unwrap();

    handle.fail();
    let err = mgr
        .execute_cleanup_for_entity("exec-1", "test", None, &strategy)
        .await
        .expect_err("cleanup without a clock reading must fail closed");
    assert!(format!("{err:?}").contains("clock unavailable"));
}

#[tokio::test]
async fn concurrent_cleanup_serialized_per_entity() {
    let storage = make_storage();
    let mgr = Arc::new(StorageBackedStateManager::<Envelope>::new(storage));

    for i in 0..8 {
        mgr.save(
            &make_envelope(
                &format!("cp-{}", i),
                None,
                None,
                1000 + i as i64,
                None,
                Some(json!({"state": i})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
    }

    let mut handles = Vec::new();
    for _ in 0..4 {
        let mgr = mgr.clone();
        handles.push(tokio::spawn(async move {
            let _result = mgr
                .execute_cleanup_for_entity(
                    "exec-1",
                    "test",
                    None,
                    &CleanupStrategy::CountBased {
                        max_checkpoints: 2,
                        min_retention: 1,
                    },
                )
                .await
                .unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let remaining = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(remaining.len(), 2, "cleanup converges to the limit");
}

#[tokio::test]
async fn cleanup_with_strategy_respects_cleanup_strategy() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    for i in 0..5 {
        mgr.save(
            &make_envelope(
                &format!("cp-{}", i),
                None,
                None,
                i as i64 * 1000,
                None,
                Some(json!({"state": i})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
    }

    let deleted = mgr
        .cleanup_with_strategy(
            "exec-1",
            &CleanupStrategy::CountBased {
                max_checkpoints: 2,
                min_retention: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(deleted, 3);

    let remaining = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(remaining.len(), 2);

    // New checkpoints created after the persisted watermark.
    for i in 5..7 {
        mgr.save(
            &make_envelope(
                &format!("cp-{}", i),
                None,
                None,
                5000 + (i - 5) as i64,
                None,
                Some(json!({"state": i})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
    }

    // Time-based strategy removes everything older than the window;
    // the latest checkpoint is always protected from deletion.
    let deleted = mgr
        .cleanup_with_strategy(
            "exec-1",
            &CleanupStrategy::TimeBased {
                max_age_seconds: 86_400,
                min_retention: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(deleted, 1);
    let remaining = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(remaining.len(), 3);
}

#[tokio::test]
async fn list_latest_by_entities_returns_newest_per_entity() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    // Multiple checkpoints per entity, interleaved timestamps.
    for (i, entity) in [(0, "exec-1"), (0, "exec-2"), (1, "exec-1")] {
        mgr.save(
            &make_envelope(
                &format!("cp-{}-{}", entity, i),
                None,
                None,
                1000 + i as i64,
                None,
                Some(json!({"state": i})),
            ),
            "test",
            entity,
        )
        .await
        .unwrap();
    }
    // Unrelated entity must not leak into the IN query.
    mgr.save(
        &make_envelope(
            "cp-other-0",
            None,
            None,
            9000,
            None,
            Some(json!({"state": "x"})),
        ),
        "test",
        "exec-3",
    )
    .await
    .unwrap();

    let latest = mgr
        .list_latest_by_entities(&["exec-1".to_string(), "exec-2".to_string()])
        .await
        .unwrap();

    assert_eq!(latest.len(), 2);
    let by_entity: std::collections::HashMap<_, _> = latest
        .into_iter()
        .map(|m| (m.entity_id.clone(), m.id.clone()))
        .collect();
    assert_eq!(
        by_entity.get("exec-1").map(String::as_str),
        Some("cp-exec-1-1")
    );
    assert_eq!(
        by_entity.get("exec-2").map(String::as_str),
        Some("cp-exec-2-0")
    );
}

#[tokio::test]
async fn list_latest_by_parent_returns_a_child_whose_newest_row_is_a_delta() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);
    let parent = json!({"hierarchy": {"path": "/parent-1/child-/"}});

    mgr.save(
        &make_envelope("child-full", None, None, 1000, None, Some(parent.clone())),
        "workflow_execution",
        "child-1",
    )
    .await
    .unwrap();
    mgr.save(
        &make_envelope(
            "child-delta",
            Some(CheckpointType::Delta),
            Some("child-full"),
            2000,
            Some(json!({"state": "s1"})),
            None,
        ),
        "workflow_execution",
        "child-1",
    )
    .await
    .unwrap();
    // A second child whose newest row is a full checkpoint.
    mgr.save(
        &make_envelope("other-full", None, None, 1500, None, Some(parent)),
        "workflow_execution",
        "child-2",
    )
    .await
    .unwrap();

    let children = mgr.list_latest_by_parent("parent-1").await.unwrap();
    let by_entity: std::collections::HashMap<_, _> = children
        .into_iter()
        .map(|m| (m.entity_id.clone(), m.id.clone()))
        .collect();
    assert_eq!(by_entity.len(), 2);
    assert_eq!(
        by_entity.get("child-1").map(String::as_str),
        Some("child-delta"),
        "the newest row is a delta, which carries no snapshot and so no parent link"
    );
    assert_eq!(
        by_entity.get("child-2").map(String::as_str),
        Some("other-full")
    );
}

#[tokio::test]
async fn list_latest_by_parent_ignores_checkpoints_of_other_parents() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    mgr.save(
        &make_envelope(
            "mine",
            None,
            None,
            1000,
            None,
            Some(json!({"hierarchy": {"path": "/parent-1/child-1/"}})),
        ),
        "workflow_execution",
        "child-1",
    )
    .await
    .unwrap();
    mgr.save(
        &make_envelope(
            "theirs",
            None,
            None,
            2000,
            None,
            Some(json!({"hierarchy": {"path": "/parent-2/child-2/"}})),
        ),
        "workflow_execution",
        "child-2",
    )
    .await
    .unwrap();

    let children = mgr.list_latest_by_parent("parent-1").await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].entity_id, "child-1");
}

#[tokio::test]
async fn metadata_chain_info_round_trip() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    mgr.save(
        &make_envelope(
            "full-1",
            None,
            None,
            1000,
            None,
            Some(json!({"state": "base"})),
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();
    mgr.save(
        &make_envelope(
            "delta-1",
            Some(CheckpointType::Delta),
            Some("full-1"),
            2000,
            Some(json!({"state": "s1"})),
            None,
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();

    let all = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(all.len(), 2);

    let full_meta = &all[0];
    assert_eq!(full_meta.chain_root_id, Some("full-1".to_string()));
    assert_eq!(full_meta.chain_position, Some(0));
    assert!(full_meta.blob_size.unwrap_or(0) > 0);

    let delta_meta = &all[1];
    assert_eq!(delta_meta.chain_root_id, Some("full-1".to_string()));
    assert_eq!(delta_meta.chain_position, Some(1));
    assert!(delta_meta.blob_size.unwrap_or(0) > 0);
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

#[tokio::test]
async fn compact_delta_chain_merges_and_fixes_successor() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    mgr.save(
        &make_envelope(
            "full-1",
            None,
            None,
            1000,
            None,
            Some(json!({"state": "base"})),
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();
    mgr.save(
        &make_envelope(
            "delta-1",
            Some(CheckpointType::Delta),
            Some("full-1"),
            2000,
            Some(json!({"state": "mid"})),
            None,
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();
    mgr.save(
        &make_envelope(
            "delta-2",
            Some(CheckpointType::Delta),
            Some("delta-1"),
            3000,
            Some(json!({"state": "final"})),
            None,
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();

    let merged = mgr
        .compact_delta_chain("exec-1", "test", &FullStateDiff, 1)
        .await
        .unwrap();
    assert_eq!(merged, 1);

    assert!(mgr.load("delta-1").await.unwrap().is_none());

    let successor = mgr.load("delta-2").await.unwrap().unwrap();
    assert_eq!(successor.previous_checkpoint_id, Some("full-1".to_string()));
    assert_eq!(successor.delta, Some(json!({"state": "final"})));

    let successor_meta = CheckpointLoader::load_metadata(&mgr, "delta-2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(successor_meta.chain_root_id, Some("full-1".to_string()));
    assert_eq!(successor_meta.chain_position, Some(1));

    let restored = FullStateDiff
        .apply_delta(&json!({"state": "base"}), successor.delta.as_ref().unwrap())
        .await
        .unwrap();
    assert_eq!(restored, json!({"state": "final"}));
}

#[tokio::test]
async fn compact_delta_chain_merges_multiple_pairs() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    mgr.save(
        &make_envelope(
            "full-1",
            None,
            None,
            1000,
            None,
            Some(json!({"state": "0"})),
        ),
        "test",
        "exec-1",
    )
    .await
    .unwrap();
    for i in 1..=4 {
        let id = format!("delta-{}", i);
        let prev = if i == 1 {
            "full-1".to_string()
        } else {
            format!("delta-{}", i - 1)
        };
        mgr.save(
            &make_envelope(
                &id,
                Some(CheckpointType::Delta),
                Some(&prev),
                (1000 + i * 100) as i64,
                Some(json!({"state": i})),
                None,
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
    }

    let merged = mgr
        .compact_delta_chain("exec-1", "test", &FullStateDiff, 2)
        .await
        .unwrap();
    assert_eq!(merged, 2);

    let all = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(all.len(), 3);

    let last = mgr.load("delta-4").await.unwrap().unwrap();
    assert_eq!(last.delta, Some(json!({"state": 4})));
    assert_eq!(last.previous_checkpoint_id, Some("delta-3".to_string()));

    let restored = FullStateDiff
        .apply_delta(&json!({"state": 0}), last.delta.as_ref().unwrap())
        .await
        .unwrap();
    assert_eq!(restored, json!({"state": 4}));
}

#[test]
fn parse_storage_metadata_accepts_status_case_variants() {
    let meta = json!({
        "entityType": "test",
        "checkpointType": "DELTA",
        "timestamp": 1000,
        "status": "CORRUPTED",
    });
    let parsed = parse_storage_metadata("cp-1", "exec-1", &meta).unwrap();
    assert_eq!(
        parsed.status,
        wf_types::checkpoint::CheckpointStatus::Corrupted
    );
    assert_eq!(parsed.checkpoint_type, CheckpointType::Delta);

    let mixed = json!({
        "entityType": "test",
        "checkpointType": "delta",
        "timestamp": 1000,
        "status": "Completed",
    });
    let parsed = parse_storage_metadata("cp-2", "exec-1", &mixed).unwrap();
    assert_eq!(
        parsed.status,
        wf_types::checkpoint::CheckpointStatus::Completed
    );
}

#[test]
fn parse_storage_metadata_rejects_missing_timestamp_and_unknown_status() {
    let missing = json!({
        "entityType": "test",
        "checkpointType": "full",
        "status": "completed",
    });
    assert!(parse_storage_metadata("cp-x", "exec-1", &missing).is_err());

    let unknown = json!({
        "entityType": "test",
        "checkpointType": "full",
        "timestamp": 1000,
        "status": "bogus-status",
    });
    assert!(parse_storage_metadata("cp-y", "exec-1", &unknown).is_err());

    let absent_status_defaults_to_completed = json!({
        "entityType": "test",
        "checkpointType": "full",
        "timestamp": 1000,
    });
    let parsed =
        parse_storage_metadata("cp-z", "exec-1", &absent_status_defaults_to_completed).unwrap();
    assert_eq!(
        parsed.status,
        wf_types::checkpoint::CheckpointStatus::Completed
    );
}
