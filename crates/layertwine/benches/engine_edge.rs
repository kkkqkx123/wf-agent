use std::path::PathBuf;

use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::types::SourceType;
use layertwine::engine::diff::diff_to_line_diff;
use layertwine::engine::merge::{apply_deltas, merge_texts};

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

fn generate_modified_text_seeded(base: &str, change_rate: f64, seed: u64) -> String {
    let mut result = String::new();
    let mut rng = seed;

    for line in base.lines() {
        rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
        let random = (rng % 100) as f64 / 100.0;

        if random < change_rate {
            result.push_str(&format!("MODIFIED_{}: {}\n", seed, line));
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

// ============================================================================
// Group 1: apply_deltas with empty delta chain (identity operation)
// ============================================================================

fn bench_apply_deltas_empty(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 1000, 10000] {
        let content = generate_test_text(lines);
        c.bench_function(&format!("apply_deltas_empty_{}_lines", lines), |b| {
            b.iter(|| apply_deltas(&content, &[]));
        });
    }
}

// ============================================================================
// Group 2: apply_deltas with single delta
// ============================================================================

fn bench_apply_deltas_single(c: &mut criterion::Criterion) {
    for &(lines, change_rate) in &[(10, 0.1), (100, 0.1), (1000, 0.1), (10000, 0.05)] {
        let base = generate_test_text(lines);
        let modified = generate_modified_text(&base, change_rate);
        let delta = create_delta_from_texts(&base, &modified);

        c.bench_function(
            &format!(
                "apply_deltas_single_{}_lines_{}percent",
                lines,
                (change_rate * 100.0) as usize
            ),
            |b| b.iter(|| apply_deltas(&base, &[delta.clone()])),
        );
    }
}

// ============================================================================
// Group 3: apply_deltas with long delta chain (sequential edits)
// ============================================================================

fn bench_apply_deltas_long_chain(c: &mut criterion::Criterion) {
    for &(lines, num_deltas) in &[(10, 10), (100, 10), (100, 50), (500, 10)] {
        let base = generate_test_text(lines);
        let mut deltas = Vec::new();
        let mut current = base.clone();

        for _ in 0..num_deltas {
            let next = generate_modified_text(&current, 0.1);
            deltas.push(create_delta_from_texts(&current, &next));
            current = next;
        }

        c.bench_function(
            &format!(
                "apply_deltas_chain_{}_lines_{}_deltas",
                lines, num_deltas
            ),
            |b| b.iter(|| apply_deltas(&base, &deltas)),
        );
    }
}

// ============================================================================
// Group 4: apply_deltas with high change rate (many modifications)
// ============================================================================

fn bench_apply_deltas_high_change(c: &mut criterion::Criterion) {
    for &change_rate in &[0.3, 0.5, 0.9] {
        let lines = 100;
        let base = generate_test_text(lines);
        let modified = generate_modified_text(&base, change_rate);
        let delta = create_delta_from_texts(&base, &modified);

        c.bench_function(
            &format!(
                "apply_deltas_high_change_100_lines_{}percent",
                (change_rate * 100.0) as usize
            ),
            |b| b.iter(|| apply_deltas(&base, &[delta.clone()])),
        );
    }
}

// ============================================================================
// Group 5: apply_deltas with complex multi-hunk diffs
// ============================================================================

fn bench_apply_deltas_multi_hunk(c: &mut criterion::Criterion) {
    for &lines in &[100, 500] {
        // Create text with modifications at multiple locations to generate multi-hunk diff
        let base = generate_test_text(lines);
        let mut modified = base.clone();
        // Modify at several specific positions to force multiple hunks
        let positions: Vec<usize> = (0..lines).step_by(lines / 5).collect();
        for &pos in &positions {
            let line_num = pos;
            modified = modified.replace(
                &format!("line {}\n", line_num),
                &format!("CHANGED line {}\n", line_num),
            );
        }
        let delta = create_delta_from_texts(&base, &modified);

        c.bench_function(
            &format!("apply_deltas_multi_hunk_{}_lines", lines),
            |b| b.iter(|| apply_deltas(&base, &[delta.clone()])),
        );
    }
}

// ============================================================================
// Group 6: merge_texts with no conflicts (non-overlapping edits)
// ============================================================================

fn bench_merge_texts_no_conflict(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 500, 1000] {
        let base = generate_test_text(lines);

        // Ours modifies first half, theirs modifies second half → no conflict
        let ours = {
            let mut t = String::new();
            for i in 0..lines / 2 {
                t.push_str(&format!("OURS line {}\n", i));
            }
            for i in lines / 2..lines {
                t.push_str(&format!("line {}\n", i));
            }
            t
        };
        let theirs = {
            let mut t = String::new();
            for i in 0..lines / 2 {
                t.push_str(&format!("line {}\n", i));
            }
            for i in lines / 2..lines {
                t.push_str(&format!("THEIRS line {}\n", i));
            }
            t
        };

        c.bench_function(&format!("merge_texts_no_conflict_{}_lines", lines), |b| {
            b.iter(|| {
                let (_, conflicts) = merge_texts(&base, &ours, &theirs);
                let _ = conflicts;
            });
        });
    }
}

// ============================================================================
// Group 7: merge_texts with conflicts (overlapping edits)
// ============================================================================

fn bench_merge_texts_with_conflicts(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 500] {
        let base = generate_test_text(lines);
        // Both sides modify the same lines but differently → genuine conflicts
        let ours = generate_modified_text_seeded(&base, 0.2, 12345);
        let theirs = generate_modified_text_seeded(&base, 0.2, 67890);

        c.bench_function(
            &format!("merge_texts_with_conflicts_{}_lines", lines),
            |b| {
                b.iter(|| {
                    let (_, conflicts) = merge_texts(&base, &ours, &theirs);
                    let _ = conflicts;
                });
            },
        );
    }
}

// ============================================================================
// Group 8: merge_texts where one side is unchanged
// ============================================================================

fn bench_merge_texts_one_side_unchanged(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 1000] {
        let base = generate_test_text(lines);
        let theirs = generate_modified_text(&base, 0.3);

        c.bench_function(
            &format!("merge_texts_one_side_unchanged_{}_lines", lines),
            |b| {
                b.iter(|| {
                    let (_, conflicts) = merge_texts(&base, &base, &theirs);
                    let _ = conflicts;
                });
            },
        );
    }
}

// ============================================================================
// Group 9: merge_texts with empty base
// ============================================================================

fn bench_merge_texts_empty_base(c: &mut criterion::Criterion) {
    for &lines in &[10, 100, 500] {
        let ours = generate_test_text(lines);
        let theirs = generate_modified_text(&ours, 0.3);

        c.bench_function(&format!("merge_texts_empty_base_{}_lines", lines), |b| {
            b.iter(|| {
                let (_, conflicts) = merge_texts("", &ours, &theirs);
                let _ = conflicts;
            });
        });
    }
}

// ============================================================================
// Group 12: apply_deltas with single-character content
// ============================================================================

fn bench_apply_deltas_extreme_small(c: &mut criterion::Criterion) {
    let contents = ["x", "", "a\nb\nc\n"];
    for &content in &contents {
        c.bench_function(
            &format!("apply_deltas_extreme_small_{:?}", content),
            |b| b.iter(|| apply_deltas(content, &[])),
        );
    }
}

// ============================================================================
// Criterion group definitions
// ============================================================================

criterion::criterion_group!(
    apply_deltas_empty_group,
    bench_apply_deltas_empty
);

criterion::criterion_group!(
    apply_deltas_single_group,
    bench_apply_deltas_single
);

criterion::criterion_group!(
    apply_deltas_chain_group,
    bench_apply_deltas_long_chain,
    bench_apply_deltas_high_change,
    bench_apply_deltas_multi_hunk
);

criterion::criterion_group!(
    merge_texts_group,
    bench_merge_texts_no_conflict,
    bench_merge_texts_with_conflicts,
    bench_merge_texts_one_side_unchanged,
    bench_merge_texts_empty_base
);

criterion::criterion_group!(
    edge_extreme_small,
    bench_apply_deltas_extreme_small
);

criterion::criterion_main!(
    apply_deltas_empty_group,
    apply_deltas_single_group,
    apply_deltas_chain_group,
    merge_texts_group,
    edge_extreme_small
);