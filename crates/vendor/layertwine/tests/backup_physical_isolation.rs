//! Test to verify physical isolation of backup module
//!
//! This test demonstrates that backups are completely independent from core storage,
//! following the design requirement for complete physical isolation.

use layertwine::backup::backup_repo::BackupRepo;
use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::{LineDiff, SnapshotId, SourceType};
use layertwine::storage::repository::{DeltaStore, FileNodeStore, SnapshotStore};
use layertwine::storage::SqliteStorage;
use std::path::PathBuf;

fn create_snapshot_in_core(
    store: &SqliteStorage,
    path: &str,
    content: &[u8],
    source_type: SourceType,
) -> SnapshotId {
    let file_node = FileNode::new(PathBuf::from(path), content);
    store.store_file_node(&file_node, content).unwrap();

    let diff = LineDiff::new(vec![]);
    let delta = Delta::new(file_node.clone(), diff, source_type);
    store.store_delta(&delta).unwrap();

    let snapshot = Snapshot::new_initial(file_node, delta.id);
    store.store_snapshot(&snapshot, content).unwrap();

    snapshot.id
}

#[test]
fn test_physical_isolation_file_content_stored_in_backup() {
    let core = SqliteStorage::new_in_memory().unwrap();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    // Create a snapshot in core storage
    let content = b"This is test content that should be stored in backup";
    let snap_id = create_snapshot_in_core(&core, "test.txt", content, SourceType::Manual);

    // Backup the snapshot
    let backup_id = backup_repo
        .backup_snapshot(&core, snap_id, Some("test-backup".to_string()))
        .unwrap();

    // Retrieve the backup
    let backup = backup_repo.get_backup(&backup_id).unwrap();

    // Verify that file_content is stored in the backup
    assert_eq!(backup.file_content, content);
}

#[test]
fn test_physical_isolation_backup_independent_of_core() {
    let core = SqliteStorage::new_in_memory().unwrap();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    // Create and backup a snapshot
    let original_content = b"Original content";
    let snap_id = create_snapshot_in_core(&core, "file.txt", original_content, SourceType::Manual);
    let backup_id = backup_repo
        .backup_snapshot(&core, snap_id, Some("isolation-test".to_string()))
        .unwrap();

    // Verify backup has the content
    let backup = backup_repo.get_backup(&backup_id).unwrap();
    assert_eq!(backup.file_content, original_content);

    // Now simulate "deleting" the core storage by creating a new one
    let _core_new = SqliteStorage::new_in_memory().unwrap();

    // Backup should still be accessible with its content
    let backup_reloaded = backup_repo.get_backup(&backup_id).unwrap();
    assert_eq!(backup_reloaded.file_content, original_content);
}

#[test]
fn test_physical_isolation_backup_survives_core_modification() {
    let core = SqliteStorage::new_in_memory().unwrap();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    // Create and backup a snapshot
    let original_content = b"Original content";
    let snap_id = create_snapshot_in_core(&core, "file.txt", original_content, SourceType::Manual);
    let backup_id = backup_repo
        .backup_snapshot(&core, snap_id, Some("survival-test".to_string()))
        .unwrap();

    // Create a modified version in core storage
    let modified_content = b"Modified content";
    create_snapshot_in_core(&core, "file.txt", modified_content, SourceType::Manual);

    // Backup should still have the original content
    let backup = backup_repo.get_backup(&backup_id).unwrap();
    assert_eq!(backup.file_content, original_content);
    assert_ne!(backup.file_content, modified_content);
}

#[test]
fn test_physical_isolation_multiple_backups_independent() {
    let core = SqliteStorage::new_in_memory().unwrap();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    // Create multiple snapshots with different content
    let content1 = b"Content version 1";
    let content2 = b"Content version 2";
    let content3 = b"Content version 3";

    let snap_id1 = create_snapshot_in_core(&core, "file.txt", content1, SourceType::Manual);
    let snap_id2 = create_snapshot_in_core(&core, "file.txt", content2, SourceType::Manual);
    let snap_id3 = create_snapshot_in_core(&core, "file.txt", content3, SourceType::Manual);

    // Backup all snapshots
    let backup_id1 = backup_repo
        .backup_snapshot(&core, snap_id1, Some("v1".to_string()))
        .unwrap();
    let backup_id2 = backup_repo
        .backup_snapshot(&core, snap_id2, Some("v2".to_string()))
        .unwrap();
    let backup_id3 = backup_repo
        .backup_snapshot(&core, snap_id3, Some("v3".to_string()))
        .unwrap();

    // Verify each backup has its own independent content
    let backup1 = backup_repo.get_backup(&backup_id1).unwrap();
    let backup2 = backup_repo.get_backup(&backup_id2).unwrap();
    let backup3 = backup_repo.get_backup(&backup_id3).unwrap();

    assert_eq!(backup1.file_content, content1);
    assert_eq!(backup2.file_content, content2);
    assert_eq!(backup3.file_content, content3);

    // Verify they are all different
    assert_ne!(backup1.file_content, backup2.file_content);
    assert_ne!(backup2.file_content, backup3.file_content);
    assert_ne!(backup1.file_content, backup3.file_content);
}

#[test]
fn test_physical_isolation_complete_independence() {
    let core = SqliteStorage::new_in_memory().unwrap();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    // Create and backup multiple snapshots
    let mut backup_ids = Vec::new();
    for i in 0..5 {
        let content = format!("Content version {}", i).into_bytes();
        let snap_id = create_snapshot_in_core(
            &core,
            &format!("file{}.txt", i),
            &content,
            SourceType::Manual,
        );
        let backup_id = backup_repo
            .backup_snapshot(&core, snap_id, Some(format!("backup-{}", i)))
            .unwrap();
        backup_ids.push((backup_id, content));
    }

    // Delete the core storage
    drop(core);

    // Create a new core storage
    let _new_core = SqliteStorage::new_in_memory().unwrap();

    // All backups should still be accessible
    let all_backups = backup_repo
        .query_backups(&layertwine::backup::backup_snapshot::BackupFilter::new())
        .unwrap();
    assert_eq!(all_backups.len(), 5);

    // Verify each backup still has its content using the stored order
    for (backup_id, expected_content) in backup_ids {
        let backup = backup_repo.get_backup(&backup_id).unwrap();
        assert_eq!(backup.file_content, expected_content);
    }
}
