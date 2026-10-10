use super::support::*;
use crate::state::CheckpointStateManager;
use checkpoint_base::cleanup_policy::CleanupStrategy;
use serde_json::json;
use std::sync::Arc;
use wf_types::checkpoint::CheckpointType;

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
