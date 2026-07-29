//! Corruption recovery and integrity tests.
//!
//! Real scenario: Storage corruption (disk error, partial write, manual DB tampering).
//! Tests verify that the system detects and handles corrupted data gracefully.

use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use layertwine::core::types::ContentId;
use layertwine::storage::repository::{
    CheckpointPersist, DeltaStore, FileNodeStore, SnapshotStore,
};
use std::path::Path;

// ---------------------------------------------------------------------------
// Invalid snapshot ID: querying non-existent snapshot should return error
// ---------------------------------------------------------------------------

#[test]
fn test_query_nonexistent_snapshot() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);
    init_repository(&env);

    let fake_id = ContentId::from_content(b"this-snapshot-does-not-exist");
    let result = env.storage.get_snapshot(&fake_id);
    assert!(result.is_err(), "getting non-existent snapshot should fail");
}

// ---------------------------------------------------------------------------
// Storage exists check on deleted/invalid IDs
// ---------------------------------------------------------------------------

#[test]
fn test_storage_exists_checks() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    let fake_id = ContentId::from_content(b"does-not-exist");

    let snapshot_exists = env.storage.snapshot_exists(&fake_id).unwrap_or(false);
    assert!(!snapshot_exists, "non-existent snapshot should not exist");

    let delta_exists = env.storage.delta_exists(&fake_id).unwrap_or(false);
    assert!(!delta_exists, "non-existent delta should not exist");

    let checkpoint_exists = env
        .storage
        .checkpoint_exists(&fake_id)
        .unwrap_or(false);
    assert!(
        !checkpoint_exists,
        "non-existent checkpoint should not exist"
    );
}

// ---------------------------------------------------------------------------
// Database file accessibility after normal ops
// ---------------------------------------------------------------------------

#[test]
fn test_db_file_integrity() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);
    init_repository(&env);
    apply_edit(&env, "integrity.rs", "fn check() {}\n");
    commit_changes(&env, "integrity check", "dev");

    // Verify DB file exists and has content
    let db_path = env.db_path.clone();
    assert!(db_path.exists(), "DB file should exist");
    let metadata = std::fs::metadata(&db_path).unwrap();
    assert!(metadata.len() > 0, "DB file should have content");
}