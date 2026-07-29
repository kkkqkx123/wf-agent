use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::{ContentId, DiffOp, Hunk, LineDiff, SourceType};
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

fn create_test_delta(lines: usize, change_rate: f64) -> Delta {
    let old_text = generate_test_text(lines);
    let new_text = generate_modified_text(&old_text, change_rate);
    let file_node = FileNode::new(PathBuf::from("test.txt"), old_text.as_bytes());
    let diff = layertwine::engine::diff::diff_to_line_diff(&old_text, &new_text);
    Delta::new(file_node, diff, SourceType::Manual)
}

fn create_test_snapshot(lines: usize) -> Snapshot {
    let content = generate_test_text(lines).as_bytes().to_vec();
    let file_node = FileNode::new(PathBuf::from("test.txt"), &content);
    Snapshot::new_initial(file_node, ContentId([0u8; 32]))
}

fn create_complex_delta(_lines: usize, num_hunks: usize) -> Delta {
    let file_node = FileNode::new(PathBuf::from("test.txt"), b"base content\n");

    let hunks = (0..num_hunks)
        .map(|i| Hunk {
            old_start: (i * 10 + 1) as u32,
            old_len: 5,
            new_start: (i * 10 + 1) as u32,
            new_len: 5,
            ops: vec![
                DiffOp::Equal { count: 2 },
                DiffOp::Replace {
                    old_start: (i * 10 + 3) as u32,
                    old_count: 1,
                    new_start: (i * 10 + 3) as u32,
                    lines: vec![format!("replaced line {}", i + 1)],
                },
                DiffOp::Equal { count: 2 },
            ],
        })
        .collect();

    let diff = LineDiff { hunks };
    Delta::new(file_node, diff, SourceType::Manual)
}

fn benchmark_serialize_delta(
    c: &mut criterion::Criterion,
    name: &str,
    lines: usize,
    change_rate: f64,
) {
    let delta = create_test_delta(lines, change_rate);

    c.bench_function(
        &format!(
            "serialize_delta_{}_{}_lines_{}_percent",
            name,
            lines,
            (change_rate * 100.0) as usize
        ),
        |b| b.iter(|| serde_json::to_vec(&delta)),
    );
}

fn benchmark_deserialize_delta(
    c: &mut criterion::Criterion,
    name: &str,
    lines: usize,
    change_rate: f64,
) {
    let delta = create_test_delta(lines, change_rate);
    let serialized = serde_json::to_vec(&delta).unwrap();

    c.bench_function(
        &format!(
            "deserialize_delta_{}_{}_lines_{}_percent",
            name,
            lines,
            (change_rate * 100.0) as usize
        ),
        |b| b.iter(|| serde_json::from_slice::<Delta>(&serialized)),
    );
}

fn benchmark_serialize_snapshot(c: &mut criterion::Criterion, name: &str, lines: usize) {
    let snapshot = create_test_snapshot(lines);

    c.bench_function(
        &format!("serialize_snapshot_{}_{}_lines", name, lines),
        |b| b.iter(|| serde_json::to_vec(&snapshot)),
    );
}

fn benchmark_deserialize_snapshot(c: &mut criterion::Criterion, name: &str, lines: usize) {
    let snapshot = create_test_snapshot(lines);
    let serialized = serde_json::to_vec(&snapshot).unwrap();

    c.bench_function(
        &format!("deserialize_snapshot_{}_{}_lines", name, lines),
        |b| b.iter(|| serde_json::from_slice::<Snapshot>(&serialized)),
    );
}

fn benchmark_serialize_complex_delta(
    c: &mut criterion::Criterion,
    name: &str,
    lines: usize,
    num_hunks: usize,
) {
    let delta = create_complex_delta(lines, num_hunks);

    c.bench_function(
        &format!(
            "serialize_complex_delta_{}_{}_lines_{}_hunks",
            name, lines, num_hunks
        ),
        |b| b.iter(|| serde_json::to_vec(&delta)),
    );
}

fn benchmark_serialize_line_diff(
    c: &mut criterion::Criterion,
    name: &str,
    lines: usize,
    change_rate: f64,
) {
    let delta = create_test_delta(lines, change_rate);

    c.bench_function(
        &format!(
            "serialize_line_diff_{}_{}_lines_{}_percent",
            name,
            lines,
            (change_rate * 100.0) as usize
        ),
        |b| b.iter(|| serde_json::to_vec(&delta.diff)),
    );
}

fn benchmark_serialize_file_node(c: &mut criterion::Criterion, name: &str, lines: usize) {
    let content = generate_test_text(lines).as_bytes().to_vec();
    let file_node = FileNode::new(PathBuf::from("test.txt"), &content);

    c.bench_function(
        &format!("serialize_file_node_{}_{}_lines", name, lines),
        |b| b.iter(|| serde_json::to_vec(&file_node)),
    );
}

pub fn bench_serialize_delta(c: &mut criterion::Criterion) {
    benchmark_serialize_delta(c, "small", 10, 0.1);
    benchmark_serialize_delta(c, "small", 10, 0.5);
    benchmark_serialize_delta(c, "medium", 100, 0.1);
    benchmark_serialize_delta(c, "medium", 100, 0.5);
    benchmark_serialize_delta(c, "large", 1000, 0.1);
}

pub fn bench_deserialize_delta(c: &mut criterion::Criterion) {
    benchmark_deserialize_delta(c, "small", 10, 0.1);
    benchmark_deserialize_delta(c, "small", 10, 0.5);
    benchmark_deserialize_delta(c, "medium", 100, 0.1);
    benchmark_deserialize_delta(c, "medium", 100, 0.5);
    benchmark_deserialize_delta(c, "large", 1000, 0.1);
}

pub fn bench_serialize_snapshot(c: &mut criterion::Criterion) {
    benchmark_serialize_snapshot(c, "small", 10);
    benchmark_serialize_snapshot(c, "medium", 100);
    benchmark_serialize_snapshot(c, "large", 1000);
}

pub fn bench_deserialize_snapshot(c: &mut criterion::Criterion) {
    benchmark_deserialize_snapshot(c, "small", 10);
    benchmark_deserialize_snapshot(c, "medium", 100);
    benchmark_deserialize_snapshot(c, "large", 1000);
}

pub fn bench_serialize_complex_delta(c: &mut criterion::Criterion) {
    benchmark_serialize_complex_delta(c, "complex", 1000, 10);
    benchmark_serialize_complex_delta(c, "complex", 1000, 50);
    benchmark_serialize_complex_delta(c, "complex", 1000, 100);
}

pub fn bench_serialize_line_diff(c: &mut criterion::Criterion) {
    benchmark_serialize_line_diff(c, "line_diff", 100, 0.1);
    benchmark_serialize_line_diff(c, "line_diff", 100, 0.3);
    benchmark_serialize_line_diff(c, "line_diff", 100, 0.5);
}

pub fn bench_serialize_file_node(c: &mut criterion::Criterion) {
    benchmark_serialize_file_node(c, "file_node", 10);
    benchmark_serialize_file_node(c, "file_node", 100);
    benchmark_serialize_file_node(c, "file_node", 1000);
}

criterion::criterion_group!(serialize_delta, bench_serialize_delta);
criterion::criterion_group!(deserialize_delta, bench_deserialize_delta);
criterion::criterion_group!(serialize_snapshot, bench_serialize_snapshot);
criterion::criterion_group!(deserialize_snapshot, bench_deserialize_snapshot);
criterion::criterion_group!(serialize_complex_delta, bench_serialize_complex_delta);
criterion::criterion_group!(serialize_line_diff, bench_serialize_line_diff);
criterion::criterion_group!(serialize_file_node, bench_serialize_file_node);

criterion::criterion_main!(
    serialize_delta,
    deserialize_delta,
    serialize_snapshot,
    deserialize_snapshot,
    serialize_complex_delta,
    serialize_line_diff,
    serialize_file_node
);
