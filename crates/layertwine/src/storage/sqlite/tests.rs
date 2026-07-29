//! White-box unit tests for SQLite storage internals.
//!
//! These tests exercise SQLite-specific behavior that is NOT testable
//! through the trait interface alone (e.g. direct SQL, connection pooling,
//! transaction internals). All black-box CRUD tests live in
//! `tests/storage_integration.rs` to avoid duplication.

use crate::core::delta::Delta;
use crate::core::file_node::FileNode;
use crate::core::types::{ContentId, LineDiff, SourceType};
use crate::storage::repository::{
    AtomicOps, CheckpointPersist, DeltaStore, FileNodeStore, SnapshotStore,
};
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageError;
use std::path::PathBuf;

fn create_test_storage() -> SqliteStorage {
    SqliteStorage::new_full_in_memory().unwrap()
}

// ---------------------------------------------------------------------------
// White-box: atomic rollback via connection internals
// ---------------------------------------------------------------------------

#[test]
fn test_atomic_rollback() {
    let storage = create_test_storage();

    let result: Result<(), StorageError> = storage.with_atomic(|s| {
        let conn = s.conn.lock();
        conn.execute(
            "INSERT INTO layers (layer_type, partition_ids, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["test_layer", b"[]", 1000, 1000],
        )?;

        Err(StorageError::Database(
            rusqlite::Error::InvalidParameterName("rollback test".to_string()),
        ))
    });

    assert!(result.is_err());

    let conn = storage.conn.lock();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM layers WHERE layer_type = ?1",
            rusqlite::params!["test_layer"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0, "atomic rollback should leave no trace");
}

// ---------------------------------------------------------------------------
// White-box: nested atomic operations (savepoint chain)
// ---------------------------------------------------------------------------

#[test]
fn test_nested_atomic_rollback_outer_fails() {
    let storage = create_test_storage();

    let result: Result<(), StorageError> = storage.with_atomic(|s| {
        // Inner operation succeeds
        s.with_atomic(|inner| {
            let conn = inner.conn.lock();
            conn.execute(
                "INSERT INTO layers (layer_type, partition_ids, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["inner_ok", b"[]", 2000, 2000],
            )?;
            Ok(())
        })?;

        // Outer operation fails — entire transaction should roll back
        Err(StorageError::Database(
            rusqlite::Error::InvalidParameterName("outer fail".to_string()),
        ))
    });

    assert!(result.is_err(), "outer failure should propagate");

    let conn = storage.conn.lock();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM layers WHERE layer_type = ?1",
            rusqlite::params!["inner_ok"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 0,
        "inner insert should be rolled back when outer fails"
    );
}

// ---------------------------------------------------------------------------
// White-box: concurrent connection isolation
// ---------------------------------------------------------------------------

#[test]
fn test_concurrent_connections_isolated() {
    let storage1 = create_test_storage();

    let file = FileNode::new(PathBuf::from("concurrent.txt"), b"data");
    storage1.store_file_node(&file, b"data").unwrap();

    // Open a second in-memory storage (separate DB)
    let storage2 = create_test_storage();

    assert!(
        !storage2.file_node_exists(file.path_str(), &file.base_hash).unwrap_or(false),
        "separate in-memory DBs should be isolated"
    );
}

// ---------------------------------------------------------------------------
// White-box: empty delta / snapshot IDs are valid but distinguishable
// ---------------------------------------------------------------------------

#[test]
fn test_empty_delta_stored() {
    let storage = create_test_storage();
    let file = FileNode::new(PathBuf::from("empty.txt"), b"base");
    storage.store_file_node(&file, b"base").unwrap();

    let empty_diff = LineDiff::new(vec![]);
    let delta = crate::core::delta::Delta::new(file, empty_diff, SourceType::Manual);
    storage.store_delta(&delta).unwrap();

    let retrieved = storage.get_delta(&delta.id).unwrap();
    assert!(retrieved.diff.is_empty(), "empty diff should roundtrip");
}

#[test]
fn test_checkpoint_exists_false() {
    let storage = create_test_storage();
    let fake_id = ContentId::from_content(b"nope");
    assert!(!storage.checkpoint_exists(&fake_id).unwrap());
}

#[test]
fn test_delete_checkpoint_not_found() {
    let storage = create_test_storage();
    let fake_id = ContentId::from_content(b"nonexistent");
    let result = storage.delete_checkpoint(&fake_id);
    assert!(result.is_err(), "deleting non-existent checkpoint should fail");
}