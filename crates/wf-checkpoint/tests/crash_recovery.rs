//! Checkpoint storage crash / corruption tests.
//!
//! Scenarios covered against a file-backed SQLite store (each test opens and
//! drops managers so every step reads from disk, simulating a process
//! restart):
//!
//! - bit flip in the middle of a payload → integrity error, record marked
//!   `corrupted`, unusable for recovery;
//! - half-written (truncated) payload → same corruption path;
//! - kill → restart: the latest checkpoint written by a previous "process"
//!   is still readable and correct;
//! - disk full (SQLite page budget exhausted): the save fails loudly, prior
//!   checkpoints stay readable, and no partial record is left behind.

use std::sync::Arc;

use wf_checkpoint::error::CheckpointError;
use wf_checkpoint::state::CheckpointStateManager;
use wf_checkpoint::state::StorageBackedStateManager;
use wf_storage::backend::StorageBackend;
use wf_storage::store::sqlite::SqliteStorage;
use wf_types::checkpoint::BaseCheckpointCore;

type Envelope = BaseCheckpointCore<serde_json::Value, serde_json::Value>;

fn make_envelope(id: &str, timestamp: i64, payload: &str) -> Envelope {
    BaseCheckpointCore {
        id: id.to_string(),
        r#type: None,
        base_checkpoint_id: None,
        previous_checkpoint_id: None,
        delta: None,
        snapshot: Some(serde_json::json!({ "state": payload })),
        timestamp: Some(timestamp),
        metadata: None,
        format_version: None,
    }
}

async fn sqlite_backend(path: &str) -> StorageBackend {
    StorageBackend::new_sqlite(path, "checkpoints")
        .await
        .expect("sqlite store opens")
}

async fn open_manager(path: &str) -> StorageBackedStateManager<Envelope> {
    StorageBackedStateManager::new(Arc::new(sqlite_backend(path).await))
}

/// Flip one byte at `offset` of the persisted payload of `id` without
/// touching the stored hash (simulates on-disk corruption).
async fn flip_payload_byte(path: &str, id: &str, offset: usize) {
    let store = SqliteStorage::new(path, "checkpoints")
        .await
        .expect("sqlite store opens");
    let (data,): (Vec<u8>,) = sqlx::query_as("SELECT data FROM checkpoints WHERE id = ?1")
        .bind(id)
        .fetch_one(store.pool())
        .await
        .expect("payload row exists");
    assert!(offset < data.len(), "offset within payload");
    let mut corrupted = data.clone();
    corrupted[offset] ^= 0xFF;
    sqlx::query("UPDATE checkpoints SET data = ?1 WHERE id = ?2")
        .bind(corrupted)
        .bind(id)
        .execute(store.pool())
        .await
        .expect("corrupted payload persisted");
}

/// Truncate the persisted payload of `id` to half its length, keeping the
/// stored hash (simulates a half-written blob after a crash).
async fn truncate_payload(path: &str, id: &str) {
    let store = SqliteStorage::new(path, "checkpoints")
        .await
        .expect("sqlite store opens");
    let (data,): (Vec<u8>,) = sqlx::query_as("SELECT data FROM checkpoints WHERE id = ?1")
        .bind(id)
        .fetch_one(store.pool())
        .await
        .expect("payload row exists");
    let half = data.len() / 2;
    sqlx::query("UPDATE checkpoints SET data = ?1 WHERE id = ?2")
        .bind(&data[..half])
        .bind(id)
        .execute(store.pool())
        .await
        .expect("truncated payload persisted");
}

/// Deterministic pseudo-random ASCII (poorly compressible, so the stored
/// blob stays large even after gzip — needed to control page usage).
fn random_ascii(len: usize) -> String {
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        out.push((b' ' + (seed >> 33) as u8 % 95) as char);
    }
    out
}

async fn metadata_status(path: &str, id: &str) -> String {
    let store = SqliteStorage::new(path, "checkpoints")
        .await
        .expect("sqlite store opens");
    let (meta,): (String,) = sqlx::query_as("SELECT metadata FROM checkpoints WHERE id = ?1")
        .bind(id)
        .fetch_one(store.pool())
        .await
        .expect("metadata row exists");
    let value: serde_json::Value = serde_json::from_str(&meta).expect("metadata is JSON");
    value
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn bit_flip_in_payload_is_detected_and_marked_corrupted() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.db");
    let db = db.to_str().unwrap();

    // Process 1 writes a checkpoint. The payload is compressed on the save
    // path, so the flip targets the middle of the persisted blob.
    let payload = "state-".repeat(256);
    let manager = open_manager(db).await;
    manager
        .save(&make_envelope("cp-1", 1000, &payload), "workflow", "exec-1")
        .await
        .unwrap();

    // Corrupt the middle byte on disk (stored length, not input length:
    // the blob is gzip-compressed by the Auto strategy).
    let store = SqliteStorage::new(db, "checkpoints").await.unwrap();
    let (stored,): (Vec<u8>,) = sqlx::query_as("SELECT data FROM checkpoints WHERE id = ?1")
        .bind("cp-1")
        .fetch_one(store.pool())
        .await
        .expect("payload row exists");
    assert!(
        wf_checkpoint::CheckpointSerializer::is_compressed(&stored),
        "large checkpoint payload must be compressed on the save path"
    );
    flip_payload_byte(db, "cp-1", stored.len() / 2).await;

    // Process 2 (restart) tries to load it: must surface Corrupted and the
    // metadata record must be marked so queries/recovery see it unusable.
    let manager = open_manager(db).await;
    let err = manager.load("cp-1").await.unwrap_err();
    assert!(
        matches!(err, CheckpointError::Corrupted { .. }),
        "expected Corrupted, got {err:?}"
    );
    assert_eq!(metadata_status(db, "cp-1").await, "corrupted");

    // A corrupted record stays queryable (so it is visible as broken) but
    // is never handed out as usable state.
    let latest = manager.get_latest("exec-1").await.unwrap();
    assert_eq!(latest.as_ref().map(|m| m.id.as_str()), Some("cp-1"));
    assert!(manager.load("cp-1").await.is_err());
}

#[tokio::test]
async fn half_written_payload_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.db");
    let db = db.to_str().unwrap();

    let manager = open_manager(db).await;
    manager
        .save(
            &make_envelope("cp-1", 1000, &"state-".repeat(128)),
            "workflow",
            "exec-1",
        )
        .await
        .unwrap();

    // Crash mid-write: only half the blob hit disk.
    truncate_payload(db, "cp-1").await;

    let manager = open_manager(db).await;
    let err = manager.load("cp-1").await.unwrap_err();
    assert!(
        matches!(err, CheckpointError::Corrupted { .. }),
        "expected Corrupted for half-written blob, got {err:?}"
    );
    assert_eq!(metadata_status(db, "cp-1").await, "corrupted");
}

#[tokio::test]
async fn restart_reads_latest_checkpoint_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.db");
    let db = db.to_str().unwrap();

    // Process 1: full + delta chain.
    {
        let manager = open_manager(db).await;
        manager
            .save(
                &make_envelope("cp-full", 1000, "base"),
                "workflow",
                "exec-1",
            )
            .await
            .unwrap();
        let mut delta = make_envelope("cp-delta", 2000, "advanced");
        delta.r#type = Some(wf_types::checkpoint::CheckpointType::Delta);
        delta.previous_checkpoint_id = Some("cp-full".to_string());
        delta.delta = Some(serde_json::json!({ "state": "advanced" }));
        manager.save(&delta, "workflow", "exec-1").await.unwrap();
    }
    // "kill" — manager dropped, pool closed.

    // Process 2 (restart): a fresh manager reads the same file.
    let manager = open_manager(db).await;
    let latest = manager
        .get_latest("exec-1")
        .await
        .unwrap()
        .expect("latest checkpoint survives restart");
    assert_eq!(latest.id, "cp-delta");

    let loaded = manager
        .load("cp-delta")
        .await
        .unwrap()
        .expect("delta checkpoint loads");
    assert_eq!(
        loaded.snapshot.as_ref().unwrap().get("state"),
        Some(&serde_json::json!("advanced"))
    );
    let chain = manager.list_by_entity("exec-1").await.unwrap();
    assert_eq!(chain.len(), 2);
}

#[tokio::test]
async fn disk_full_write_fails_visibly_and_preserves_prior_state() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let db = db_path.to_str().unwrap();

    let manager = open_manager(db).await;
    let big_payload = random_ascii(16 * 1024);
    manager
        .save(
            &make_envelope("cp-1", 1000, &big_payload),
            "workflow",
            "exec-1",
        )
        .await
        .unwrap();

    // Simulate a full disk: the database file and its WAL sidecar become
    // read-only. Permission checks happen at open time, so the failing
    // writes must come through a freshly opened pool (a "restarted" process
    // hitting a full disk). Writes fail with SQLITE_READONLY — the save
    // must fail loudly (never silently swallowed), and the failed record
    // must not appear in listings.
    for candidate in ["", "-wal", "-shm"] {
        let sidecar = dir.path().join(format!("state.db{candidate}"));
        if let Ok(meta) = std::fs::metadata(&sidecar) {
            let mut perms = meta.permissions();
            perms.set_readonly(true);
            std::fs::set_permissions(&sidecar, perms).expect("sidecar made read-only");
        }
    }
    let manager = open_manager(db).await;

    let err = manager
        .save(
            &make_envelope("cp-2", 2000, &big_payload),
            "workflow",
            "exec-1",
        )
        .await
        .unwrap_err();
    assert!(
        !matches!(err, CheckpointError::Corrupted { .. }),
        "disk-full is a write error, not corruption"
    );

    // Prior checkpoints stay readable.
    let loaded = manager.load("cp-1").await.unwrap().unwrap();
    assert_eq!(
        loaded.snapshot.as_ref().unwrap().get("state"),
        Some(&serde_json::json!(big_payload))
    );
    let list = manager.list_by_entity("exec-1").await.unwrap();
    assert_eq!(list.len(), 1, "failed save leaves no partial record");

    // Restore write access so the temp dir can be cleaned up.
    for candidate in ["", "-wal", "-shm"] {
        let sidecar = dir.path().join(format!("state.db{candidate}"));
        if let Ok(meta) = std::fs::metadata(&sidecar) {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = meta.permissions();
            perms.set_mode(0o644);
            std::fs::set_permissions(&sidecar, perms).expect("sidecar made writable again");
        }
    }
}

#[tokio::test]
async fn corrupted_checkpoint_never_poisons_later_checkpoints() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.db");
    let db = db.to_str().unwrap();

    let manager = open_manager(db).await;
    manager
        .save(
            &make_envelope("cp-1", 1000, &"x-".repeat(128)),
            "workflow",
            "exec-1",
        )
        .await
        .unwrap();
    manager
        .save(
            &make_envelope("cp-2", 2000, "healthy"),
            "workflow",
            "exec-1",
        )
        .await
        .unwrap();

    flip_payload_byte(db, "cp-1", 4).await;

    // cp-2 remains fully readable although its older sibling is corrupt.
    let manager = open_manager(db).await;
    assert!(manager.load("cp-1").await.is_err());
    let healthy = manager.load("cp-2").await.unwrap().unwrap();
    assert_eq!(
        healthy.snapshot.as_ref().unwrap().get("state"),
        Some(&serde_json::json!("healthy"))
    );
}
