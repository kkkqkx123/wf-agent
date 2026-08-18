//! Shared test utilities for #[cfg(test)] modules in src/.
//!
//! Provides commonly used setup helpers to reduce duplication across
//! test modules in `layered/`, `backup/`, and other crates.

#![allow(dead_code)]

use crate::core::delta::Delta;
use crate::core::file_node::FileNode;
use crate::core::snapshot::Snapshot;
use crate::core::types::{LineDiff, SnapshotId, SourceType};
use crate::storage::repository::{DeltaStore, FileNodeStore, SnapshotStore};
use crate::storage::SqliteStorage;

/// Create an in-memory SQLite storage for testing.
pub fn setup_storage() -> SqliteStorage {
    SqliteStorage::new_in_memory().unwrap()
}

/// Create an in-memory SQLite storage with full schema (checkpoint + branch tables).
pub fn setup_storage_full() -> SqliteStorage {
    let storage = SqliteStorage::new_in_memory().unwrap();
    storage
        .with_conn(crate::storage::migrations::initialize_full)
        .unwrap();
    storage
}

/// Create an initial snapshot with a given content and source type.
///
/// Stores a FileNode (path="test.txt"), an empty Delta, and a Snapshot
/// atomically via the storage layer.
pub fn create_initial_snapshot(
    storage: &SqliteStorage,
    content: &str,
    source_type: SourceType,
) -> SnapshotId {
    let file_node = FileNode::new(std::path::PathBuf::from("test.txt"), content.as_bytes());
    storage
        .store_file_node(&file_node, content.as_bytes())
        .unwrap();

    let empty_diff = LineDiff::new(vec![]);
    let delta = Delta::new(file_node.clone(), empty_diff, source_type);
    storage.store_delta(&delta).unwrap();

    let snapshot = Snapshot::new_initial(file_node, delta.id);
    storage.store_snapshot(&snapshot, b"").unwrap();
    snapshot.id
}
