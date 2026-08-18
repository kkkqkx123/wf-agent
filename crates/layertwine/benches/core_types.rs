use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::{ContentId, SourceType};
use layertwine::engine::diff::diff_to_line_diff;
use std::path::PathBuf;

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

fn benchmark_content_id(c: &mut criterion::Criterion, name: &str, size: usize) {
    let data = vec![0u8; size];

    c.bench_function(
        &format!("content_id_from_content_{}_{}_bytes", name, size),
        |b| b.iter(|| ContentId::from_content(&data)),
    );
}

fn benchmark_content_id_hex(c: &mut criterion::Criterion, name: &str) {
    let data = b"hello world";
    let id = ContentId::from_content(data);

    c.bench_function(&format!("content_id_to_hex_{}", name), |b| {
        b.iter(|| id.to_hex())
    });
}

fn benchmark_content_id_from_hex(c: &mut criterion::Criterion, name: &str) {
    let data = b"hello world";
    let id = ContentId::from_content(data);
    let hex = id.to_hex();

    c.bench_function(&format!("content_id_from_hex_{}", name), |b| {
        b.iter(|| ContentId::from_hex(&hex))
    });
}

fn benchmark_delta_compute_id(
    c: &mut criterion::Criterion,
    name: &str,
    lines: usize,
    change_rate: f64,
) {
    let old_text = generate_test_text(lines);
    let new_text = generate_modified_text(&old_text, change_rate);
    let file_node = FileNode::new(PathBuf::from("test.txt"), old_text.as_bytes());
    let diff = diff_to_line_diff(&old_text, &new_text);
    let delta = Delta::new(file_node, diff, SourceType::Manual);

    c.bench_function(
        &format!(
            "delta_compute_id_{}_{}_lines_{}_percent",
            name,
            lines,
            (change_rate * 100.0) as usize
        ),
        |b| b.iter(|| delta.compute_id()),
    );
}

fn benchmark_snapshot_compute_id(c: &mut criterion::Criterion, name: &str, lines: usize) {
    let old_text = generate_test_text(lines);
    let new_text = generate_modified_text(&old_text, 0.1);
    let file_node = FileNode::new(PathBuf::from("test.txt"), old_text.as_bytes());
    let diff = diff_to_line_diff(&old_text, &new_text);
    let delta = Delta::new(file_node, diff, SourceType::Manual);
    let snapshot = Snapshot::new_initial(delta.file.clone(), delta.id);

    c.bench_function(
        &format!("snapshot_compute_id_{}_{}_lines", name, lines),
        |b| b.iter(|| snapshot.compute_id()),
    );
}

fn benchmark_delta_creation(
    c: &mut criterion::Criterion,
    name: &str,
    lines: usize,
    change_rate: f64,
) {
    let old_text = generate_test_text(lines);
    let new_text = generate_modified_text(&old_text, change_rate);
    let file_node = FileNode::new(PathBuf::from("test.txt"), old_text.as_bytes());

    c.bench_function(
        &format!(
            "delta_creation_{}_{}_lines_{}_percent",
            name,
            lines,
            (change_rate * 100.0) as usize
        ),
        |b| {
            b.iter(|| {
                let diff = diff_to_line_diff(&old_text, &new_text);
                Delta::new(file_node.clone(), diff, SourceType::Manual)
            })
        },
    );
}

pub fn bench_content_id(c: &mut criterion::Criterion) {
    benchmark_content_id(c, "small", 16);
    benchmark_content_id(c, "small", 64);
    benchmark_content_id(c, "medium", 256);
    benchmark_content_id(c, "medium", 1024);
    benchmark_content_id(c, "large", 4096);
    benchmark_content_id(c, "large", 16384);
}

pub fn bench_content_id_hex(c: &mut criterion::Criterion) {
    benchmark_content_id_hex(c, "conversion");
    benchmark_content_id_from_hex(c, "conversion");
}

pub fn bench_delta_compute_id(c: &mut criterion::Criterion) {
    benchmark_delta_compute_id(c, "small", 10, 0.1);
    benchmark_delta_compute_id(c, "small", 10, 0.5);
    benchmark_delta_compute_id(c, "medium", 100, 0.1);
    benchmark_delta_compute_id(c, "medium", 100, 0.5);
    benchmark_delta_compute_id(c, "large", 1000, 0.1);
}

pub fn bench_snapshot_compute_id(c: &mut criterion::Criterion) {
    benchmark_snapshot_compute_id(c, "small", 10);
    benchmark_snapshot_compute_id(c, "medium", 100);
    benchmark_snapshot_compute_id(c, "large", 1000);
}

pub fn bench_delta_creation(c: &mut criterion::Criterion) {
    benchmark_delta_creation(c, "small", 10, 0.1);
    benchmark_delta_creation(c, "small", 10, 0.5);
    benchmark_delta_creation(c, "medium", 100, 0.1);
    benchmark_delta_creation(c, "medium", 100, 0.5);
    benchmark_delta_creation(c, "large", 1000, 0.1);
}

criterion::criterion_group!(content_id, bench_content_id);
criterion::criterion_group!(content_id_hex, bench_content_id_hex);
criterion::criterion_group!(delta_id, bench_delta_compute_id);
criterion::criterion_group!(snapshot_id, bench_snapshot_compute_id);
criterion::criterion_group!(delta_creation, bench_delta_creation);

criterion::criterion_main!(
    content_id,
    content_id_hex,
    delta_id,
    snapshot_id,
    delta_creation
);
