use super::support::*;
use crate::state::CheckpointStateManager;
use checkpoint_base::delta::{CheckpointLoader, DiffCalculator};
use serde_json::json;
use wf_types::checkpoint::CheckpointType;

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
