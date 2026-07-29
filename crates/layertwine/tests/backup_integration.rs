//! Integration tests for the backup module.
//!
//! These tests exercise the backup repository functionality end-to-end.
//! They verify:
//! - Creating and storing backups from snapshots
//! - Querying backups with filters
//! - Restoring backups with three-way merge
//! - Backup integrity and isolation
//! - Backup deletion and count

use layertwine::backup::backup_repo::BackupRepo;
use layertwine::backup::backup_snapshot::BackupFilter;
use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::{BackupId, DeltaId, LineDiff, PartitionType, SnapshotId, SourceType};
use layertwine::engine::diff::diff_to_line_diff;
use layertwine::engine::merge::apply_deltas;
use layertwine::storage::repository::{DeltaStore, FileNodeStore, PartitionStore, SnapshotStore};
use layertwine::storage::SqliteStorage;
use std::path::PathBuf;

fn setup_core_repo() -> SqliteStorage {
    SqliteStorage::new_in_memory().unwrap()
}

fn create_initial_snapshot(
    store: &SqliteStorage,
    path: &str,
    content: &[u8],
    source_type: SourceType,
) -> (FileNode, DeltaId, SnapshotId) {
    let file_node = FileNode::new(PathBuf::from(path), content);
    store.store_file_node(&file_node, content).unwrap();

    let diff = LineDiff::new(vec![]);
    let delta = Delta::new(file_node.clone(), diff, source_type);
    store.store_delta(&delta).unwrap();

    let snapshot = Snapshot::new_initial(file_node.clone(), delta.id);
    store.store_snapshot(&snapshot, content).unwrap();

    (file_node, delta.id, snapshot.id)
}

fn create_edited_snapshot(
    store: &SqliteStorage,
    parent_id: SnapshotId,
    file_node: &FileNode,
    old_text: &str,
    new_text: &str,
    partition_type: &str,
) -> (DeltaId, SnapshotId) {
    let diff = diff_to_line_diff(old_text, new_text);
    let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
    store.store_delta(&delta).unwrap();

    let parent = store.get_snapshot(&parent_id).unwrap();
    let snapshot = Snapshot::from_parent(&parent, delta.id, partition_type.to_string());
    store.store_snapshot(&snapshot, &[]).unwrap();

    (delta.id, snapshot.id)
}

fn create_staged_partition(store: &SqliteStorage, snapshot_id: SnapshotId) {
    use layertwine::core::partition::Partition;
    let partition = Partition {
        id: uuid::Uuid::now_v7(),
        name: "staged".to_string(),
        current_snapshot: snapshot_id,
        history: vec![snapshot_id],
        partition_type: PartitionType::Staged,
    };
    store.create_partition(&partition).unwrap();
}

// ---------------------------------------------------------------------------
// Test: Create and retrieve a backup
// ---------------------------------------------------------------------------

#[test]
fn test_create_and_retrieve_backup() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let snap_id = create_initial_snapshot(&core, "test.txt", b"hello world", SourceType::Manual).2;

    let backup_id = backup_repo
        .backup_snapshot(&core, snap_id, Some("test-backup".to_string()))
        .unwrap();

    let loaded = backup_repo.get_backup(&backup_id).unwrap();
    assert_eq!(loaded.source_snapshot, snap_id);
    assert_eq!(loaded.label, Some("test-backup".to_string()));
    assert_eq!(loaded.deltas.len(), 1);
}

// ---------------------------------------------------------------------------
// Test: Query backups with various filters
// ---------------------------------------------------------------------------

#[test]
fn test_query_backups_by_label() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let snap_id1 = create_initial_snapshot(&core, "a.txt", b"content a", SourceType::Manual).2;
    let snap_id2 = create_initial_snapshot(&core, "b.txt", b"content b", SourceType::Manual).2;

    backup_repo
        .backup_snapshot(&core, snap_id1, Some("label-a".to_string()))
        .unwrap();
    backup_repo
        .backup_snapshot(&core, snap_id2, Some("label-b".to_string()))
        .unwrap();

    let filtered = BackupFilter::new().with_label("label-a");
    let result = backup_repo.query_backups(&filtered).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].label, Some("label-a".to_string()));
}

#[test]
fn test_query_backups_by_source_snapshot() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let snap_id = create_initial_snapshot(&core, "test.txt", b"content", SourceType::Manual).2;
    let backup_id = backup_repo
        .backup_snapshot(&core, snap_id, Some("test".to_string()))
        .unwrap();

    let filtered = BackupFilter::new().with_source(snap_id);
    let result = backup_repo.query_backups(&filtered).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, backup_id);
}

#[test]
fn test_query_backups_all() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    for i in 0..5 {
        let snap_id = create_initial_snapshot(
            &core,
            &format!("file{}.txt", i),
            b"content",
            SourceType::Manual,
        )
        .2;
        backup_repo
            .backup_snapshot(&core, snap_id, Some(format!("backup-{}", i)))
            .unwrap();
    }

    let all = backup_repo.query_backups(&BackupFilter::new()).unwrap();
    assert_eq!(all.len(), 5);
}

// ---------------------------------------------------------------------------
// Test: Delete backup
// ---------------------------------------------------------------------------

#[test]
fn test_delete_backup() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let snap_id = create_initial_snapshot(&core, "del.txt", b"delete me", SourceType::Manual).2;
    let backup_id = backup_repo.backup_snapshot(&core, snap_id, None).unwrap();

    assert_eq!(backup_repo.count().unwrap(), 1);
    backup_repo.delete_backup(&backup_id).unwrap();
    assert_eq!(backup_repo.count().unwrap(), 0);
}

#[test]
fn test_delete_nonexistent_backup_fails() {
    let backup_repo = BackupRepo::new_in_memory().unwrap();
    let fake_id = BackupId::from_content(b"fake");

    let result = backup_repo.delete_backup(&fake_id);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: Restore backup with three-way merge
// ---------------------------------------------------------------------------

#[test]
fn test_merge_backup_to_staged() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let (file_node, _delta_id, initial_id) =
        create_initial_snapshot(&core, "file.txt", b"a\nb\nc\n", SourceType::Manual);
    create_staged_partition(&core, initial_id);

    let (_staged_delta_id, staged_id) = create_edited_snapshot(
        &core,
        initial_id,
        &file_node,
        "a\nb\nc\n",
        "a\nB\nc\n",
        "staged",
    );
    let staged_partition = core.get_partition_by_name("staged").unwrap();
    core.update_pointer(&staged_partition.id, &staged_id)
        .unwrap();

    let (_backup_delta_id, backup_snap_id) = create_edited_snapshot(
        &core,
        initial_id,
        &file_node,
        "a\nb\nc\n",
        "a\nb\nC\n",
        "manual",
    );

    let backup_id = backup_repo
        .backup_snapshot(&core, backup_snap_id, Some("merge-test".to_string()))
        .unwrap();

    let merged_id = backup_repo.merge_to_staged(&backup_id, &core).unwrap();

    let staged = core.get_partition_by_name("staged").unwrap();
    assert_eq!(staged.current_snapshot, merged_id);

    let merged_snapshot = core.get_snapshot(&merged_id).unwrap();
    assert!(merged_snapshot.parents.contains(&staged_id));
    assert!(merged_snapshot.parents.contains(&backup_snap_id));

    let merged_base = core
        .get_file_content(
            merged_snapshot.file.path_str(),
            &merged_snapshot.file.base_hash,
        )
        .unwrap();
    let merged_deltas = core.get_deltas(&merged_snapshot.deltas).unwrap();
    let merged_content =
        apply_deltas(&String::from_utf8(merged_base).unwrap(), &merged_deltas).unwrap();
    assert_eq!(merged_content, "a\nB\nC\n");
}

#[test]
fn test_restore_backup_without_conflict() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let (file_node, _delta_id, initial_id) =
        create_initial_snapshot(&core, "file.txt", b"a\nb\nc\n", SourceType::Manual);
    create_staged_partition(&core, initial_id);

    let (_delta_id, backup_snap_id) = create_edited_snapshot(
        &core,
        initial_id,
        &file_node,
        "a\nb\nc\n",
        "a\nb\nC\n",
        "manual",
    );

    let backup_id = backup_repo
        .backup_snapshot(&core, backup_snap_id, Some("restore-test".to_string()))
        .unwrap();

    let merged_id = backup_repo.merge_to_staged(&backup_id, &core).unwrap();

    let merged_snapshot = core.get_snapshot(&merged_id).unwrap();
    let merged_base = core
        .get_file_content(
            merged_snapshot.file.path_str(),
            &merged_snapshot.file.base_hash,
        )
        .unwrap();
    let merged_deltas = core.get_deltas(&merged_snapshot.deltas).unwrap();
    let merged_content =
        apply_deltas(&String::from_utf8(merged_base).unwrap(), &merged_deltas).unwrap();
    assert_eq!(merged_content, "a\nb\nC\n");
}

// ---------------------------------------------------------------------------
// Test: Backup integrity and isolation
// ---------------------------------------------------------------------------

#[test]
fn test_backup_integrity_check() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let snap_id = create_initial_snapshot(&core, "check.txt", b"integrity", SourceType::Manual).2;
    let backup_id = backup_repo.backup_snapshot(&core, snap_id, None).unwrap();

    let backup = backup_repo.get_backup(&backup_id).unwrap();
    let mut recomputed = layertwine::backup::backup_snapshot::BackupSnapshot::with_options(
        backup.source_snapshot,
        backup.file.clone(),
        backup.deltas.clone(),
        backup.label.clone(),
        backup.agent_id.clone(),
        backup.source_type.clone(),
        backup.file_content.clone(),
    );
    recomputed.backed_at = backup.backed_at;
    assert_eq!(recomputed.id, backup.id);
}

#[test]
fn test_physical_isolation() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let snap_id = create_initial_snapshot(&core, "isolated.txt", b"isolated", SourceType::Manual).2;
    backup_repo.backup_snapshot(&core, snap_id, None).unwrap();

    assert!(core.snapshot_exists(&snap_id).unwrap());
    backup_repo.delete_backup(&snap_id).unwrap_err();
    assert!(core.snapshot_exists(&snap_id).unwrap());
}

// ---------------------------------------------------------------------------
// Test: Backup with metadata
// ---------------------------------------------------------------------------

#[test]
fn test_backup_filter_by_label() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let snap_id1 = create_initial_snapshot(&core, "a.txt", b"content", SourceType::Manual).2;
    let snap_id2 = create_initial_snapshot(&core, "b.txt", b"content", SourceType::Manual).2;

    backup_repo
        .backup_snapshot(&core, snap_id1, Some("label-a".to_string()))
        .unwrap();
    backup_repo
        .backup_snapshot(&core, snap_id2, Some("label-b".to_string()))
        .unwrap();

    let filtered = BackupFilter::new().with_label("label-a");
    let result = backup_repo.query_backups(&filtered).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].label, Some("label-a".to_string()));
}

#[test]
fn test_backup_filter_by_time_range() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let snap_id = create_initial_snapshot(&core, "test.txt", b"content", SourceType::Manual).2;

    let backup_id = backup_repo
        .backup_snapshot(&core, snap_id, Some("test".to_string()))
        .unwrap();

    let _backup = backup_repo.get_backup(&backup_id).unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    let one_hour_ago = now - 3600 * 1000;
    let one_hour_later = now + 3600 * 1000;

    let filtered = BackupFilter::new().with_time_range(one_hour_ago, one_hour_later);
    let result = backup_repo.query_backups(&filtered).unwrap();
    assert!(!result.is_empty());
}

// ---------------------------------------------------------------------------
// Test: Complex merge scenarios
// ---------------------------------------------------------------------------

#[test]
fn test_merge_with_conflict() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let (file_node, _delta_id, initial_id) =
        create_initial_snapshot(&core, "file.txt", b"a\nb\nc\n", SourceType::Manual);
    create_staged_partition(&core, initial_id);

    let (_staged_delta_id, staged_id) = create_edited_snapshot(
        &core,
        initial_id,
        &file_node,
        "a\nb\nc\n",
        "a\nX\nc\n",
        "staged",
    );
    let staged_partition = core.get_partition_by_name("staged").unwrap();
    core.update_pointer(&staged_partition.id, &staged_id)
        .unwrap();

    let (_backup_delta_id, backup_snap_id) = create_edited_snapshot(
        &core,
        initial_id,
        &file_node,
        "a\nb\nc\n",
        "a\nY\nc\n",
        "manual",
    );

    let backup_id = backup_repo
        .backup_snapshot(&core, backup_snap_id, Some("conflict-test".to_string()))
        .unwrap();

    let merged_id = backup_repo.merge_to_staged(&backup_id, &core).unwrap();

    let merged_snapshot = core.get_snapshot(&merged_id).unwrap();
    assert!(merged_snapshot.parents.contains(&staged_id));
    assert!(merged_snapshot.parents.contains(&backup_snap_id));
}

#[test]
fn test_merge_multi_file_edits() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let (file_node, _delta_id, initial_id) =
        create_initial_snapshot(&core, "file.txt", b"a\nb\nc\nd\n", SourceType::Manual);
    create_staged_partition(&core, initial_id);

    let (_staged_delta_id, staged_id) = create_edited_snapshot(
        &core,
        initial_id,
        &file_node,
        "a\nb\nc\nd\n",
        "a\nB\nc\nd\n",
        "staged",
    );
    let staged_partition = core.get_partition_by_name("staged").unwrap();
    core.update_pointer(&staged_partition.id, &staged_id)
        .unwrap();

    let (_backup_delta_id, backup_snap_id) = create_edited_snapshot(
        &core,
        initial_id,
        &file_node,
        "a\nb\nc\nd\n",
        "a\nb\nC\nd\n",
        "manual",
    );

    let backup_id = backup_repo
        .backup_snapshot(&core, backup_snap_id, Some("multi-edit-test".to_string()))
        .unwrap();

    let merged_id = backup_repo.merge_to_staged(&backup_id, &core).unwrap();

    let merged_snapshot = core.get_snapshot(&merged_id).unwrap();
    let merged_base = core
        .get_file_content(
            merged_snapshot.file.path_str(),
            &merged_snapshot.file.base_hash,
        )
        .unwrap();
    let merged_deltas = core.get_deltas(&merged_snapshot.deltas).unwrap();
    let merged_content =
        apply_deltas(&String::from_utf8(merged_base).unwrap(), &merged_deltas).unwrap();
    assert_eq!(merged_content, "a\nB\nC\nd\n");
}

// ---------------------------------------------------------------------------
// Test: Edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_backup_empty_delta_chain() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let snap_id = create_initial_snapshot(&core, "empty.txt", b"content", SourceType::Manual).2;
    let backup_id = backup_repo.backup_snapshot(&core, snap_id, None).unwrap();

    let backup = backup_repo.get_backup(&backup_id).unwrap();
    assert!(!backup.deltas.is_empty());
}

#[test]
fn test_multiple_backups_same_snapshot() {
    let core = setup_core_repo();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let snap_id = create_initial_snapshot(&core, "multi.txt", b"content", SourceType::Manual).2;

    let backup_id1 = backup_repo
        .backup_snapshot(&core, snap_id, Some("backup-1".to_string()))
        .unwrap();
    let backup_id2 = backup_repo
        .backup_snapshot(&core, snap_id, Some("backup-2".to_string()))
        .unwrap();

    assert_ne!(backup_id1, backup_id2);
    assert_eq!(backup_repo.count().unwrap(), 2);

    let backup1 = backup_repo.get_backup(&backup_id1).unwrap();
    let backup2 = backup_repo.get_backup(&backup_id2).unwrap();
    assert_eq!(backup1.source_snapshot, backup2.source_snapshot);
}
