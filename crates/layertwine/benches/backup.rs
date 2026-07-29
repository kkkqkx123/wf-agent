use std::path::PathBuf;

use layertwine::backup::backup_repo::BackupRepo;
use layertwine::backup::backup_snapshot::BackupFilter;
use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::partition::Partition;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::{LineDiff, PartitionType, SourceType};
use layertwine::engine::diff::diff_to_line_diff;
use layertwine::storage::repository::{DeltaStore, FileNodeStore, PartitionStore, SnapshotStore};
use layertwine::storage::SqliteStorage;

// ============================================================================
// Shared helpers
// ============================================================================

fn generate_test_text(lines: usize) -> String {
    (0..lines).map(|i| format!("line {}\n", i)).collect()
}

fn generate_modified_text(base: &str, change_rate: f64) -> String {
    let mut result = String::new();
    let mut rng = 0u64;

    for line in base.lines() {
        rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
        let random = (rng % 100) as f64 / 100.0;

        if random < change_rate {
            result.push_str(&format!("MODIFIED: {}\n", line));
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

fn setup_storage_and_snapshot(
    lines: usize,
    change_rate: f64,
) -> (SqliteStorage, BackupRepo, layertwine::core::types::SnapshotId) {
    let storage = SqliteStorage::new_in_memory().unwrap();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let content = generate_test_text(lines);
    let modified = generate_modified_text(&content, change_rate);

    let file_node = FileNode::new(PathBuf::from("test.txt"), content.as_bytes());
    storage
        .store_file_node(&file_node, content.as_bytes())
        .unwrap();

    let diff = diff_to_line_diff(&content, &modified);
    let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
    storage.store_delta(&delta).unwrap();

    let snapshot = Snapshot::new_initial(file_node, delta.id);
    storage.store_snapshot(&snapshot, content.as_bytes()).unwrap();

    (storage, backup_repo, snapshot.id)
}

fn setup_storage_with_partition(
    lines: usize,
) -> (SqliteStorage, BackupRepo, layertwine::core::types::SnapshotId) {
    let storage = SqliteStorage::new_in_memory().unwrap();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    let content = generate_test_text(lines);

    let file_node = FileNode::new(PathBuf::from("test.txt"), content.as_bytes());
    storage
        .store_file_node(&file_node, content.as_bytes())
        .unwrap();

    let diff = LineDiff::new(vec![]);
    let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
    storage.store_delta(&delta).unwrap();

    let snapshot = Snapshot::new_initial(file_node, delta.id);
    storage.store_snapshot(&snapshot, content.as_bytes()).unwrap();

    // Create staged partition
    let partition = Partition {
        id: uuid::Uuid::now_v7(),
        name: "staged".to_string(),
        current_snapshot: snapshot.id,
        history: vec![snapshot.id],
        partition_type: PartitionType::Staged,
    };
    storage.create_partition(&partition).unwrap();

    (storage, backup_repo, snapshot.id)
}

// ============================================================================
// Group 1: Backup snapshot operations
// ============================================================================

fn bench_backup_snapshot(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 1000] {
        c.bench_function(&format!("backup_snapshot_{}_lines", lines), |b| {
            b.iter_with_setup(
                || setup_storage_and_snapshot(lines, 0.1),
                |(storage, backup_repo, snap_id)| {
                    let _ = backup_repo.backup_snapshot(&storage, snap_id, None);
                },
            );
        });
    }
}

fn bench_backup_snapshot_with_label(c: &mut criterion::Criterion) {
    for &lines in &[10, 100] {
        c.bench_function(&format!("backup_snapshot_with_label_{}_lines", lines), |b| {
            b.iter_with_setup(
                || setup_storage_and_snapshot(lines, 0.1),
                |(storage, backup_repo, snap_id)| {
                    let _ = backup_repo.backup_snapshot(
                        &storage,
                        snap_id,
                        Some("performance-test-backup".to_string()),
                    );
                },
            );
        });
    }
}

// ============================================================================
// Group 2: Get backup operations
// ============================================================================

fn bench_get_backup(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 1000] {
        c.bench_function(&format!("get_backup_{}_lines", lines), |b| {
            // Setup once, benchmark get
            let (storage, backup_repo, snap_id) = setup_storage_and_snapshot(lines, 0.1);
            let backup_id = backup_repo
                .backup_snapshot(&storage, snap_id, None)
                .unwrap();

            b.iter(|| {
                let _ = backup_repo.get_backup(&backup_id);
            });
        });
    }
}

// ============================================================================
// Group 3: Query backups by filter
// ============================================================================

fn bench_query_backups(c: &mut criterion::Criterion) {
    for &count in &[10, 50, 100] {
        c.bench_function(&format!("query_backups_{}_entries_all", count), |b| {
            let storage = SqliteStorage::new_in_memory().unwrap();
            let backup_repo = BackupRepo::new_in_memory().unwrap();

            for i in 0..count {
                let content = generate_test_text(10);
                let file_node = FileNode::new(
                    PathBuf::from(format!("file_{}.txt", i)),
                    content.as_bytes(),
                );
                storage
                    .store_file_node(&file_node, content.as_bytes())
                    .unwrap();
                let diff = LineDiff::new(vec![]);
                let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
                storage.store_delta(&delta).unwrap();
                let snapshot = Snapshot::new_initial(file_node, delta.id);
                storage
                    .store_snapshot(&snapshot, content.as_bytes())
                    .unwrap();

                let label = if i % 2 == 0 {
                    Some("even".to_string())
                } else {
                    Some("odd".to_string())
                };
                backup_repo
                    .backup_snapshot(&storage, snapshot.id, label)
                    .unwrap();
            }

            b.iter(|| {
                let _ = backup_repo.query_backups(&BackupFilter::new());
            });
        });
    }
}

fn bench_query_backups_filtered(c: &mut criterion::Criterion) {
    let count = 100;
    let storage = SqliteStorage::new_in_memory().unwrap();
    let backup_repo = BackupRepo::new_in_memory().unwrap();

    for i in 0..count {
        let content = generate_test_text(10);
        let file_node = FileNode::new(
            PathBuf::from(format!("file_{}.txt", i)),
            content.as_bytes(),
        );
        storage
            .store_file_node(&file_node, content.as_bytes())
            .unwrap();
        let diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();
        let snapshot = Snapshot::new_initial(file_node, delta.id);
        storage
            .store_snapshot(&snapshot, content.as_bytes())
            .unwrap();

        let label = if i % 2 == 0 {
            Some("even".to_string())
        } else {
            Some("odd".to_string())
        };
        backup_repo
            .backup_snapshot(&storage, snapshot.id, label)
            .unwrap();
    }

    c.bench_function("query_backups_filtered_by_label", |b| {
        b.iter(|| {
            let filter = BackupFilter::new().with_label("even");
            let _ = backup_repo.query_backups(&filter);
        });
    });
}

// ============================================================================
// Group 4: Merge backup into staged (restore)
// ============================================================================

fn bench_merge_to_staged(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 500] {
        c.bench_function(&format!("merge_to_staged_{}_lines", lines), |b| {
            let (storage, backup_repo, initial_id) = setup_storage_with_partition(lines);

            // Create a modified snapshot that simulates backup source
            let base_content = generate_test_text(lines);
            let modified_content = generate_modified_text(&base_content, 0.2);

            let file_node = FileNode::new(PathBuf::from("test.txt"), base_content.as_bytes());
            let diff = diff_to_line_diff(&base_content, &modified_content);
            let delta = Delta::new(file_node.clone(), diff, SourceType::Backup);
            storage.store_delta(&delta).unwrap();
            let backup_snapshot = Snapshot::from_parent(
                &storage.get_snapshot(&initial_id).unwrap(),
                delta.id,
                "backup".to_string(),
            );
            storage
                .store_snapshot(&backup_snapshot, &[])
                .unwrap();

            let backup_id = backup_repo
                .backup_snapshot(&storage, backup_snapshot.id, None)
                .unwrap();

            b.iter_with_setup(
                || {
                    // Before each iteration: reset staged pointer back to initial
                    let staged = storage.get_partition_by_name("staged").unwrap();
                    storage
                        .update_pointer(&staged.id, &initial_id)
                        .unwrap();
                    backup_id
                },
                |bid| {
                    let _ = backup_repo.merge_to_staged(&bid, &storage);
                },
            );
        });
    }
}

// ============================================================================
// Group 5: Delete backup
// ============================================================================

fn bench_delete_backup(c: &mut criterion::Criterion) {
    for &count in &[1, 10, 50] {
        c.bench_function(&format!("delete_backup_single_from_{}", count), |b| {
            let storage = SqliteStorage::new_in_memory().unwrap();
            let backup_repo = BackupRepo::new_in_memory().unwrap();
            let mut backup_ids = Vec::new();

            for i in 0..count {
                let content = generate_test_text(10);
                let file_node = FileNode::new(
                    PathBuf::from(format!("del_{}.txt", i)),
                    content.as_bytes(),
                );
                storage
                    .store_file_node(&file_node, content.as_bytes())
                    .unwrap();
                let diff = LineDiff::new(vec![]);
                let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
                storage.store_delta(&delta).unwrap();
                let snapshot = Snapshot::new_initial(file_node, delta.id);
                storage
                    .store_snapshot(&snapshot, content.as_bytes())
                    .unwrap();
                let bid = backup_repo
                    .backup_snapshot(&storage, snapshot.id, None)
                    .unwrap();
                backup_ids.push(bid);
            }

            let target = backup_ids[0];
            b.iter(|| {
                let _ = backup_repo.delete_backup(&target);
            });
        });
    }
}

fn bench_backup_count(c: &mut criterion::Criterion) {
    for &count in &[10, 50, 100] {
        c.bench_function(&format!("backup_count_{}_entries", count), |b| {
            let storage = SqliteStorage::new_in_memory().unwrap();
            let backup_repo = BackupRepo::new_in_memory().unwrap();

            for i in 0..count {
                let content = generate_test_text(10);
                let file_node = FileNode::new(
                    PathBuf::from(format!("count_{}.txt", i)),
                    content.as_bytes(),
                );
                storage
                    .store_file_node(&file_node, content.as_bytes())
                    .unwrap();
                let diff = LineDiff::new(vec![]);
                let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
                storage.store_delta(&delta).unwrap();
                let snapshot = Snapshot::new_initial(file_node, delta.id);
                storage
                    .store_snapshot(&snapshot, content.as_bytes())
                    .unwrap();
                backup_repo
                    .backup_snapshot(&storage, snapshot.id, None)
                    .unwrap();
            }

            b.iter(|| {
                let _ = backup_repo.count();
            });
        });
    }
}

// ============================================================================
// Criterion group definitions
// ============================================================================

criterion::criterion_group!(
    backup_snapshot_ops,
    bench_backup_snapshot,
    bench_backup_snapshot_with_label
);

criterion::criterion_group!(backup_get_ops, bench_get_backup);

criterion::criterion_group!(
    backup_query_ops,
    bench_query_backups,
    bench_query_backups_filtered
);

criterion::criterion_group!(backup_restore_ops, bench_merge_to_staged);

criterion::criterion_group!(
    backup_admin_ops,
    bench_delete_backup,
    bench_backup_count
);

criterion::criterion_main!(
    backup_snapshot_ops,
    backup_get_ops,
    backup_query_ops,
    backup_restore_ops,
    backup_admin_ops
);