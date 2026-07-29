use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::types::{DiffOp, Hunk, LineDiff, SourceType};
use layertwine::engine::diff::diff_to_line_diff;
use layertwine::engine::merge::apply_deltas;
use layertwine::engine::merge::merge_texts;
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

fn create_delta_from_texts(old: &str, new: &str) -> Delta {
    let file_node = FileNode::new(PathBuf::from("test.txt"), old.as_bytes());
    let diff = diff_to_line_diff(old, new);
    Delta::new(file_node, diff, SourceType::Manual)
}

#[allow(dead_code)]
fn create_manual_delta(old_start: u32, insert_lines: Vec<String>) -> Delta {
    let file_node = FileNode::new(PathBuf::from("test.txt"), b"base content\n");
    let diff = LineDiff {
        hunks: vec![Hunk {
            old_start,
            old_len: 0,
            new_start: old_start,
            new_len: insert_lines.len() as u32,
            ops: vec![DiffOp::Insert {
                new_start: old_start,
                lines: insert_lines,
            }],
        }],
    };
    Delta::new(file_node, diff, SourceType::Manual)
}

fn benchmark_apply_deltas(
    c: &mut criterion::Criterion,
    name: &str,
    lines: usize,
    num_deltas: usize,
    change_rate: f64,
) {
    let base_text = generate_test_text(lines);
    let mut deltas = Vec::new();

    let mut current_text = base_text.clone();
    for _ in 0..num_deltas {
        let new_text = generate_modified_text(&current_text, change_rate);
        deltas.push(create_delta_from_texts(&current_text, &new_text));
        current_text = new_text;
    }

    c.bench_function(
        &format!(
            "apply_deltas_{}_{}_deltas_{}_lines_{}_percent",
            name,
            num_deltas,
            lines,
            (change_rate * 100.0) as usize
        ),
        |b| b.iter(|| apply_deltas(&base_text, &deltas)),
    );
}

fn benchmark_merge_texts(c: &mut criterion::Criterion, name: &str, lines: usize) {
    let base = generate_test_text(lines);
    let left = generate_modified_text(&base, 0.1);
    let right = generate_modified_text(&base, 0.1);

    c.bench_function(&format!("merge_texts_{}_{}_lines", name, lines), |b| {
        b.iter(|| {
            let (result, conflicts) = merge_texts(&base, &left, &right);
            let _ = result;
            let _ = conflicts;
        })
    });
}

pub fn bench_apply_deltas_small(c: &mut criterion::Criterion) {
    benchmark_apply_deltas(c, "small", 10, 1, 0.1);
    benchmark_apply_deltas(c, "small", 10, 5, 0.1);
    benchmark_apply_deltas(c, "small", 10, 10, 0.1);
}

pub fn bench_apply_deltas_medium(c: &mut criterion::Criterion) {
    benchmark_apply_deltas(c, "medium", 100, 1, 0.1);
    benchmark_apply_deltas(c, "medium", 100, 5, 0.1);
    benchmark_apply_deltas(c, "medium", 100, 10, 0.1);
}

pub fn bench_apply_deltas_large(c: &mut criterion::Criterion) {
    benchmark_apply_deltas(c, "large", 1000, 1, 0.1);
    benchmark_apply_deltas(c, "large", 1000, 5, 0.1);
}

pub fn bench_merge_texts(c: &mut criterion::Criterion) {
    benchmark_merge_texts(c, "merge", 10);
    benchmark_merge_texts(c, "merge", 100);
    benchmark_merge_texts(c, "merge", 500);
    benchmark_merge_texts(c, "merge", 1000);
}

criterion::criterion_group!(apply_deltas_small, bench_apply_deltas_small);
criterion::criterion_group!(apply_deltas_medium, bench_apply_deltas_medium);
criterion::criterion_group!(apply_deltas_large, bench_apply_deltas_large);
criterion::criterion_group!(merge_operations, bench_merge_texts);

criterion::criterion_main!(
    apply_deltas_small,
    apply_deltas_medium,
    apply_deltas_large,
    merge_operations
);
