//! Headless throughput probe: preparation cache and Markdown steady rows.
//!
//! Not a criterion benchmark by design; it reuses the existing demo examples
//! setup and prints per-frame microseconds for steady content, so regressions
//! show up in normal example runs without new benchmark infrastructure.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_bench
//! ```

use std::time::Instant;

use wf_tui::markdown::document::parse_document;
use wf_tui::markdown::styled::render_styled_lines;
use wf_tui::prep_cache::PreparedScrollback;
use wf_tui::transcript::{HistoryLine, LineState, Role};

fn sample_lines(count: usize) -> Vec<HistoryLine> {
    (0..count)
        .map(|i| {
            HistoryLine::new_with_role(
                format!("assistant answer row {i} with enough words to wrap across columns"),
                LineState::Committed,
                Role::Default,
            )
        })
        .collect()
}

fn main() {
    const WIDTH: u16 = 80;
    const FRAMES: usize = 200;

    let full = sample_lines(500);
    let mut cache = PreparedScrollback::new();
    let start = Instant::now();
    cache.sync_replace(&full, WIDTH, 1);
    let replace_us = start.elapsed().as_micros();
    println!("prep_cache sync_replace 500 rows: {replace_us}us total");

    let start = Instant::now();
    let mut laid_out = 0usize;
    for frame in 0..FRAMES {
        let mut next = full.clone();
        next.push(HistoryLine::new_with_role(
            format!("steady tail frame {frame}"),
            LineState::Committed,
            Role::Default,
        ));
        cache.sync_append(&next, WIDTH, 2 + frame as u64);
        laid_out += cache.last_layout_count();
    }
    let elapsed = start.elapsed();
    println!(
        "prep_cache steady append: {FRAMES} frames in {}us ({:.2}us/frame, laid_out={laid_out})",
        elapsed.as_micros(),
        elapsed.as_micros() as f64 / FRAMES as f64,
    );

    let src = (0..200)
        .map(|i| format!("- item {i} with **bold** and `code` plus trailing words"))
        .collect::<Vec<_>>()
        .join("\n");
    let doc = parse_document(&src);
    let _ = doc;
    let start = Instant::now();
    for _ in 0..FRAMES {
        let lines = render_styled_lines(&src, WIDTH);
        std::hint::black_box(lines.len());
    }
    let elapsed = start.elapsed();
    println!(
        "markdown steady styled rows: {FRAMES} frames in {}us ({:.2}us/frame)",
        elapsed.as_micros(),
        elapsed.as_micros() as f64 / FRAMES as f64,
    );
}
