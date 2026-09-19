use layertwine::engine::diff::{diff_to_line_diff, format_unified_diff};

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

fn benchmark_diff(c: &mut criterion::Criterion, name: &str, lines: usize, change_rate: f64) {
    let old_text = generate_test_text(lines);
    let new_text = generate_modified_text(&old_text, change_rate);

    c.bench_function(
        &format!(
            "diff_to_line_diff_{}_{}_lines_{}_percent",
            name,
            lines,
            (change_rate * 100.0) as usize
        ),
        |b| b.iter(|| diff_to_line_diff(&old_text, &new_text)),
    );
}

fn benchmark_unified_diff(
    c: &mut criterion::Criterion,
    name: &str,
    lines: usize,
    change_rate: f64,
) {
    let old_text = generate_test_text(lines);
    let new_text = generate_modified_text(&old_text, change_rate);

    c.bench_function(
        &format!(
            "format_unified_diff_{}_{}_lines_{}_percent",
            name,
            lines,
            (change_rate * 100.0) as usize
        ),
        |b| b.iter(|| format_unified_diff(&old_text, &new_text, 3)),
    );
}

pub fn bench_diff_small(c: &mut criterion::Criterion) {
    benchmark_diff(c, "small", 10, 0.1);
}

pub fn bench_diff_medium(c: &mut criterion::Criterion) {
    benchmark_diff(c, "medium", 100, 0.1);
    benchmark_diff(c, "medium", 100, 0.3);
    benchmark_diff(c, "medium", 100, 0.5);
}

pub fn bench_diff_large(c: &mut criterion::Criterion) {
    benchmark_diff(c, "large", 1000, 0.1);
    benchmark_diff(c, "large", 1000, 0.3);
    benchmark_diff(c, "large", 1000, 0.5);
}

pub fn bench_diff_huge(c: &mut criterion::Criterion) {
    benchmark_diff(c, "huge", 10000, 0.1);
    benchmark_diff(c, "huge", 10000, 0.3);
}

pub fn bench_unified_diff(c: &mut criterion::Criterion) {
    benchmark_unified_diff(c, "unified", 100, 0.3);
    benchmark_unified_diff(c, "unified", 1000, 0.3);
}

criterion::criterion_group!(diff_small, bench_diff_small);
criterion::criterion_group!(diff_medium, bench_diff_medium);
criterion::criterion_group!(diff_large, bench_diff_large);
criterion::criterion_group!(diff_huge, bench_diff_huge);
criterion::criterion_group!(unified, bench_unified_diff);

criterion::criterion_main!(diff_small, diff_medium, diff_large, diff_huge, unified);
