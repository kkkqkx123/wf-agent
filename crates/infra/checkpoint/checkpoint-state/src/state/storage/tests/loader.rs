use super::support::*;
use crate::state::CheckpointStateManager;
use checkpoint_base::delta::CheckpointLoader;
use checkpoint_base::error::CheckpointError;
use serde_json::json;
use wf_storage::domain::store::Store;
use wf_types::checkpoint::CheckpointType;

#[tokio::test]
async fn load_checkpoint_data_returns_raw_stored_bytes() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage.clone());
    let envelope = make_envelope(
        "full-1",
        None,
        None,
        1000,
        None,
        Some(json!({"state": "base"})),
    );
    mgr.save(&envelope, "test", "exec-1").await.unwrap();

    // The raw bytes are the checkpoint's own serialization: they must
    // round-trip to exactly the saved envelope, undecoded on the way out.
    let raw = mgr
        .load_checkpoint_data("full-1")
        .await
        .unwrap()
        .expect("raw bytes stored");
    let decoded: Envelope = serde_json::from_slice(&raw).unwrap();
    assert_eq!(decoded, envelope);

    assert!(mgr.load_checkpoint_data("missing").await.unwrap().is_none());
}

#[tokio::test]
async fn load_metadata_parses_indexed_fields() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage.clone());
    mgr.save(
        &make_envelope("full-1", None, None, 1000, None, Some(json!({"state": "base"}))),
        "agent_loop",
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
        "agent_loop",
        "exec-1",
    )
    .await
    .unwrap();

    let meta = CheckpointLoader::load_metadata(&mgr, "delta-1")
        .await
        .unwrap()
        .expect("metadata");
    assert_eq!(meta.id, "delta-1");
    assert_eq!(meta.entity_type, "agent_loop");
    assert_eq!(meta.entity_id, "exec-1");
    assert_eq!(meta.checkpoint_type, CheckpointType::Delta);
    assert_eq!(meta.timestamp, 2000);
    assert_eq!(meta.previous_checkpoint_id.as_deref(), Some("full-1"));
    assert_eq!(meta.base_checkpoint_id.as_deref(), Some("full-1"));

    assert!(CheckpointLoader::load_metadata(&mgr, "missing").await.unwrap().is_none());
}

#[tokio::test]
async fn load_metadata_defaults_unknown_entity_and_rejects_missing_timestamp() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage.clone());

    // A raw row without an entityId falls back to "unknown".
    storage
        .save(
            "raw-1",
            b"raw-payload",
            &json!({"timestamp": 1234, "entityType": "raw"}),
        )
        .await
        .unwrap();
    let meta = CheckpointLoader::load_metadata(&mgr, "raw-1")
        .await
        .unwrap()
        .expect("metadata");
    assert_eq!(meta.entity_id, "unknown");
    assert_eq!(meta.entity_type, "raw");
    assert_eq!(meta.checkpoint_type, CheckpointType::Full);

    // A row without a timestamp is corrupted, never defaulted.
    storage
        .save("raw-2", b"raw-payload", &json!({"entityType": "raw"}))
        .await
        .unwrap();
    let err = CheckpointLoader::load_metadata(&mgr, "raw-2").await.unwrap_err();
    assert!(matches!(err, CheckpointError::Corrupted { .. }));
}
