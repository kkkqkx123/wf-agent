use std::path::PathBuf;

use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::types::{ContentId, SourceType};
use layertwine::engine::diff::diff_to_line_diff;
use layertwine::storage::repository::{DeltaStore, FileNodeStore, SnapshotStore};
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

// ============================================================================
// Group 1: store_file_node with various content sizes
// ============================================================================

fn bench_store_file_node(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 1000, 10000] {
        let content = generate_test_text(lines).as_bytes().to_vec();

        c.bench_function(&format!("store_file_node_{}_lines", lines), |b| {
            b.iter_with_setup(
                || {
                    let storage = SqliteStorage::new_in_memory().unwrap();
                    (storage, content.clone())
                },
                |(storage, content)| {
                    let file_node = FileNode::new(PathBuf::from("test.txt"), &content);
                    let _ = storage.store_file_node(&file_node, &content);
                },
            );
        });
    }
}

// ============================================================================
// Group 2: get_file_content with various content sizes
// ============================================================================

fn bench_get_file_content(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 1000, 10000] {
        let content = generate_test_text(lines).as_bytes().to_vec();
        let storage = SqliteStorage::new_in_memory().unwrap();
        let file_node = FileNode::new(PathBuf::from("test.txt"), &content);
        storage
            .store_file_node(&file_node, &content)
            .unwrap();
        let hash = file_node.base_hash;

        c.bench_function(&format!("get_file_content_{}_lines", lines), |b| {
            b.iter(|| {
                let _ = storage.get_file_content("test.txt", &hash);
            });
        });
    }
}

// ============================================================================
// Group 3: file_node_exists
// ============================================================================

fn bench_file_node_exists(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 1000] {
        let content = generate_test_text(lines).as_bytes().to_vec();
        let storage = SqliteStorage::new_in_memory().unwrap();
        let file_node = FileNode::new(PathBuf::from("test.txt"), &content);
        storage
            .store_file_node(&file_node, &content)
            .unwrap();
        let hash = file_node.base_hash;

        c.bench_function(&format!("file_node_exists_true_{}_lines", lines), |b| {
            b.iter(|| {
                let _ = storage.file_node_exists("test.txt", &hash);
            });
        });

        c.bench_function(&format!("file_node_exists_false_{}_lines", lines), |b| {
            let fake_hash = [0u8; 32];
            b.iter(|| {
                let _ = storage.file_node_exists("nonexistent.txt", &fake_hash);
            });
        });
    }
}

// ============================================================================
// Group 4: store_delta with various content sizes (different change rates)
// ============================================================================

fn bench_store_delta_size(c: &mut criterion::Criterion) {
    for &(lines, change_rate) in &[(10, 0.1), (100, 0.1), (1000, 0.1), (10000, 0.05)] {
        let base = generate_test_text(lines);
        let modified = generate_modified_text(&base, change_rate);
        let file_node = FileNode::new(PathBuf::from("test.txt"), base.as_bytes());
        let diff = diff_to_line_diff(&base, &modified);
        let delta = Delta::new(file_node, diff, SourceType::Manual);

        c.bench_function(
            &format!(
                "store_delta_{}_lines_{}percent",
                lines,
                (change_rate * 100.0) as usize
            ),
            |b| {
                b.iter_with_setup(
                    || SqliteStorage::new_in_memory().unwrap(),
                    |storage| {
                        let _ = storage.store_delta(&delta);
                    },
                );
            },
        );
    }
}

// ============================================================================
// Group 5: get_deltas batch (many deltas in one batch call)
// ============================================================================

fn bench_get_deltas_batch(c: &mut criterion::Criterion) {
    for &batch_size in &[1, 10, 50, 100] {
        let storage = SqliteStorage::new_in_memory().unwrap();
        let mut delta_ids = Vec::new();

        for i in 0..batch_size {
            let base = generate_test_text(10);
            let modified = generate_modified_text(&base, 0.2);
            let file_node = FileNode::new(
                PathBuf::from(format!("file_{}.txt", i)),
                base.as_bytes(),
            );
            let diff = diff_to_line_diff(&base, &modified);
            let delta = Delta::new(file_node, diff, SourceType::Manual);
            let id = delta.id;
            storage.store_delta(&delta).unwrap();
            delta_ids.push(id);
        }

        c.bench_function(&format!("get_deltas_batch_{}_deltas", batch_size), |b| {
            b.iter(|| {
                let _ = storage.get_deltas(&delta_ids);
            });
        });
    }
}

// ============================================================================
// Group 6: Combined operation: store_file_node + store_delta + store_snapshot
// ============================================================================

fn bench_store_snapshot_chain(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 1000] {
        c.bench_function(&format!("store_snapshot_chain_{}_lines", lines), |b| {
            b.iter_with_setup(
                || {
                    let storage = SqliteStorage::new_in_memory().unwrap();
                    let content = generate_test_text(lines).as_bytes().to_vec();
                    (storage, content)
                },
                |(storage, content)| {
                    let file_node = FileNode::new(PathBuf::from("chain.txt"), &content);
                    storage.store_file_node(&file_node, &content).unwrap();
                    let diff = layertwine::core::types::LineDiff::new(vec![]);
                    let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
                    storage.store_delta(&delta).unwrap();
                    let snapshot =
                        layertwine::core::snapshot::Snapshot::new_initial(file_node, delta.id);
                    storage.store_snapshot(&snapshot, &content).unwrap();
                },
            );
        });
    }
}

// ============================================================================
// Group 7: ContentId operations with varying data sizes
// ============================================================================

fn bench_content_id_large_data(c: &mut criterion::Criterion) {
    for &size in &[1024, 65536, 524288] {
        let data = vec![0xABu8; size];
        c.bench_function(&format!("content_id_from_content_{}KB", size / 1024), |b| {
            b.iter(|| ContentId::from_content(&data));
        });
    }
}

// ============================================================================
// Group 8: store_file_node with varying file path lengths
// ============================================================================

fn bench_store_file_node_path_length(c: &mut criterion::Criterion) {
    let content = b"test content for path benchmark";
    let path_lengths = [
        ("short", "a.txt"),
        ("medium", "src/main/java/com/example/Test.java"),
        (
            "long",
            "src/main/java/com/example/very/deeply/nested/directory/structure/TestFile.java",
        ),
    ];

    for &(name, path) in &path_lengths {
        c.bench_function(&format!("store_file_node_path_{}", name), |b| {
            b.iter_with_setup(
                || SqliteStorage::new_in_memory().unwrap(),
                |storage| {
                    let file_node = FileNode::new(PathBuf::from(path), content);
                    let _ = storage.store_file_node(&file_node, content);
                },
            );
        });
    }
}

// ============================================================================
// Group 9: Multiple stores to same file path (overwrite pattern)
// ============================================================================

fn bench_store_file_node_same_path(c: &mut criterion::Criterion) {
    let content = b"same path content";
    let storage = SqliteStorage::new_in_memory().unwrap();

    c.bench_function("store_file_node_same_path_10_times", |b| {
        b.iter(|| {
            for _ in 0..10 {
                let mut content_vec = content.to_vec();
                content_vec.push(rand_byte());
                let file_node = FileNode::new(PathBuf::from("same.txt"), &content_vec);
                let _ = storage.store_file_node(&file_node, &content_vec);
            }
        });
    });
}

fn rand_byte() -> u8 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u8
}

// ============================================================================
// Criterion group definitions
// ============================================================================

criterion::criterion_group!(
    file_node_store,
    bench_store_file_node,
    bench_store_file_node_path_length,
    bench_store_file_node_same_path
);

criterion::criterion_group!(
    file_node_read,
    bench_get_file_content,
    bench_file_node_exists
);

criterion::criterion_group!(
    delta_ops,
    bench_store_delta_size,
    bench_get_deltas_batch
);

criterion::criterion_group!(
    composite_ops,
    bench_store_snapshot_chain
);

criterion::criterion_group!(
    content_id_large,
    bench_content_id_large_data
);

criterion::criterion_main!(
    file_node_store,
    file_node_read,
    delta_ops,
    composite_ops,
    content_id_large
);