use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::SourceType;
use layertwine::storage::repository::{DeltaStore, FileNodeStore, SnapshotStore};
use layertwine::storage::sqlite::SqliteStorage;
use tempfile::TempDir;

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

fn create_delta(file_path: &str, content: &[u8]) -> Delta {
    use layertwine::engine::diff::diff_to_line_diff;

    let old_text = std::str::from_utf8(content).unwrap();
    let new_text = generate_modified_text(old_text, 0.1);
    let file_node = FileNode::new(std::path::PathBuf::from(file_path), content);
    let diff = diff_to_line_diff(old_text, &new_text);
    Delta::new(file_node, diff, SourceType::Manual)
}

fn create_snapshot(file_path: &str, content: &[u8]) -> Snapshot {
    let old_text = std::str::from_utf8(content).unwrap();
    let new_text = generate_modified_text(old_text, 0.1);
    let file_node = FileNode::new(std::path::PathBuf::from(file_path), content);

    let diff = layertwine::engine::diff::diff_to_line_diff(old_text, &new_text);
    let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);

    Snapshot::new_initial(file_node, delta.id)
}

fn benchmark_store_snapshot(c: &mut criterion::Criterion, name: &str, lines: usize) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new(&db_path).unwrap();

    let content = generate_test_text(lines).as_bytes().to_vec();
    let snapshot = create_snapshot("test.txt", &content);

    c.bench_function(&format!("store_snapshot_{}_{}_lines", name, lines), |b| {
        b.iter(|| storage.store_snapshot(&snapshot, &content))
    });
}

fn benchmark_store_delta(c: &mut criterion::Criterion, name: &str, lines: usize) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new(&db_path).unwrap();

    let content = generate_test_text(lines).as_bytes().to_vec();
    let delta = create_delta("test.txt", &content);

    c.bench_function(&format!("store_delta_{}_{}_lines", name, lines), |b| {
        b.iter(|| storage.store_delta(&delta))
    });
}

fn benchmark_get_snapshot(c: &mut criterion::Criterion, name: &str, lines: usize) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new(&db_path).unwrap();

    let content = generate_test_text(lines).as_bytes().to_vec();
    let snapshot = create_snapshot("test.txt", &content);
    storage.store_snapshot(&snapshot, &content).unwrap();
    let snapshot_id = snapshot.id;

    c.bench_function(&format!("get_snapshot_{}_{}_lines", name, lines), |b| {
        b.iter(|| storage.get_snapshot(&snapshot_id))
    });
}

fn benchmark_get_delta(c: &mut criterion::Criterion, name: &str, lines: usize) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new(&db_path).unwrap();

    let content = generate_test_text(lines).as_bytes().to_vec();
    let delta = create_delta("test.txt", &content);
    storage.store_delta(&delta).unwrap();
    let delta_id = delta.id;

    c.bench_function(&format!("get_delta_{}_{}_lines", name, lines), |b| {
        b.iter(|| storage.get_delta(&delta_id))
    });
}

fn benchmark_batch_store_snapshots(
    c: &mut criterion::Criterion,
    name: &str,
    batch_size: usize,
    lines: usize,
) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new(&db_path).unwrap();

    let mut snapshots_with_content = Vec::new();
    for i in 0..batch_size {
        let content = generate_test_text(lines).as_bytes().to_vec();
        let snapshot = create_snapshot(&format!("test{}.txt", i), &content);
        snapshots_with_content.push((snapshot, content));
    }

    let snapshots_ref: Vec<_> = snapshots_with_content
        .iter()
        .map(|(s, c)| (s, c.as_slice()))
        .collect();

    c.bench_function(
        &format!(
            "batch_store_snapshots_{}_{}_batch_{}_lines",
            name, batch_size, lines
        ),
        |b| b.iter(|| storage.store_snapshots_batch(&snapshots_ref)),
    );
}

fn benchmark_store_file_node(c: &mut criterion::Criterion, name: &str, lines: usize) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new(&db_path).unwrap();

    let content = generate_test_text(lines).as_bytes().to_vec();
    let file_node = FileNode::new(std::path::PathBuf::from("test.txt"), &content);

    c.bench_function(&format!("store_file_node_{}_{}_lines", name, lines), |b| {
        b.iter(|| storage.store_file_node(&file_node, &content))
    });
}

pub fn bench_store_snapshot(c: &mut criterion::Criterion) {
    benchmark_store_snapshot(c, "small", 10);
    benchmark_store_snapshot(c, "medium", 100);
    benchmark_store_snapshot(c, "large", 1000);
}

pub fn bench_store_delta(c: &mut criterion::Criterion) {
    benchmark_store_delta(c, "small", 10);
    benchmark_store_delta(c, "medium", 100);
    benchmark_store_delta(c, "large", 1000);
}

pub fn bench_get_snapshot(c: &mut criterion::Criterion) {
    benchmark_get_snapshot(c, "small", 10);
    benchmark_get_snapshot(c, "medium", 100);
    benchmark_get_snapshot(c, "large", 1000);
}

pub fn bench_get_delta(c: &mut criterion::Criterion) {
    benchmark_get_delta(c, "small", 10);
    benchmark_get_delta(c, "medium", 100);
    benchmark_get_delta(c, "large", 1000);
}

pub fn bench_batch_store_snapshots(c: &mut criterion::Criterion) {
    benchmark_batch_store_snapshots(c, "small", 10, 10);
    benchmark_batch_store_snapshots(c, "small", 50, 10);
    benchmark_batch_store_snapshots(c, "medium", 10, 100);
    benchmark_batch_store_snapshots(c, "medium", 50, 100);
}

pub fn bench_store_file_node(c: &mut criterion::Criterion) {
    benchmark_store_file_node(c, "small", 10);
    benchmark_store_file_node(c, "medium", 100);
    benchmark_store_file_node(c, "large", 1000);
}

criterion::criterion_group!(store_snapshot, bench_store_snapshot);
criterion::criterion_group!(store_delta, bench_store_delta);
criterion::criterion_group!(get_snapshot, bench_get_snapshot);
criterion::criterion_group!(get_delta, bench_get_delta);
criterion::criterion_group!(batch_store_snapshots, bench_batch_store_snapshots);
criterion::criterion_group!(store_file_node, bench_store_file_node);

criterion::criterion_main!(
    store_snapshot,
    store_delta,
    get_snapshot,
    get_delta,
    batch_store_snapshots,
    store_file_node
);
