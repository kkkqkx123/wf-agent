//! Terminal-free scrollback layout, wrapping and baseline replay.

use crate::frame_metrics::{FrameMetric, FrameMetrics};
use crate::width::{ColumnWidth, WidthMeasure};

use super::test_model::TestRenderModel;
use super::view::RenderView;

/// Viewport geometry probe: the visible display-row window for a scrollback
/// of `total_rows` with `scroll` rows held above the tail.
pub fn viewport_window(total_rows: usize, height: usize, scroll: usize) -> (usize, usize) {
    let height = height.max(1);
    let max_scroll = total_rows.saturating_sub(height);
    let scroll = scroll.min(max_scroll);
    let start = max_scroll.saturating_sub(scroll);
    (start, start.saturating_add(height).min(total_rows))
}

/// Lay out a view without a terminal: wrap history plus the streaming tail
/// to `width` and return the visible rows for a `height`-tall viewport.
/// Wrapping is grapheme-safe and mirrors the scrollback tail-follow rule.
///
/// This is a layout probe, not the event-summary kernel in
/// [`crate::headless::HeadlessRenderer`]: that kernel turns execution events
/// into stdout/diag text, while this function only wraps committed rows for
/// geometry and budget assertions.
pub fn render_headless(model: &impl RenderView, width: u16, height: usize) -> Vec<String> {
    let w = usize::from(width.max(1));
    let mut rows: Vec<String> = Vec::new();
    for index in 0..model.history_count() {
        if let Some(line) = model.history_line(index) {
            rows.extend(wrap_plain(line, w));
        }
    }
    let streaming = model.streaming_text();
    if !streaming.is_empty() {
        rows.extend(wrap_plain(streaming, w));
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    let (start, end) = viewport_window(rows.len(), height, model.view_scroll());
    rows[start..end].to_vec()
}

/// Lay out a view without a terminal through the neutral document model:
/// each history row is parsed to a document, flattened to plain text, then
/// wrapped. The plain-text view stays the ground truth, so this path must
/// agree with [`render_headless`] on the same input.
pub fn render_headless_document(model: &impl RenderView, width: u16, height: usize) -> Vec<String> {
    let w = usize::from(width.max(1));
    let mut rows: Vec<String> = Vec::new();
    for index in 0..model.history_count() {
        if let Some(line) = model.history_line(index) {
            let doc = tui_markdown::markdown::document::parse_document(line);
            let plain = tui_markdown::markdown::document::document_plain_text(&doc);
            let source = if plain.is_empty() {
                line.to_string()
            } else {
                plain
            };
            for part in source.split('\n') {
                rows.extend(wrap_plain(part, w));
            }
        }
    }
    let streaming = model.streaming_text();
    if !streaming.is_empty() {
        rows.extend(wrap_plain(streaming, w));
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    let (start, end) = viewport_window(rows.len(), height, model.view_scroll());
    rows[start..end].to_vec()
}

/// Replay a fixed model sequence headlessly and collect per-frame metrics.
/// Each frame lays out the visible rows and samples the streaming bytes as
/// the parse signal, so the returned report is a deterministic baseline for
/// optimization comparisons: rerun the same sequence after a change and
/// diff the reports.
pub fn replay_baseline(models: &[TestRenderModel], width: u16, height: usize) -> FrameMetrics {
    let mut metrics = FrameMetrics::new();
    for (frame, model) in models.iter().enumerate() {
        let rows = render_headless(model, width, height);
        let parsed = RenderView::streaming_text(model).len()
            + model.history.iter().map(|line| line.len()).sum::<usize>();
        metrics.record(FrameMetric::new(frame as u64, 0, 0, 0, rows.len(), parsed));
    }
    metrics
}

/// Fixed representative frame sequence for baseline reports: empty, two
/// streaming-growth frames, a commit, a scrolled frame and a settled tail.
/// Rerun [`replay_baseline`] over this sequence before and after an
/// optimization and diff the reports.
pub fn fixed_baseline_sequence(width: u16) -> Vec<TestRenderModel> {
    let mut empty = TestRenderModel::new(width);
    empty.now_ms = 0;
    let mut growing = TestRenderModel::new(width);
    growing.push_history("alpha line one");
    growing.set_streaming("live tail one");
    growing.now_ms = 100;
    let mut grown = growing.clone();
    grown.set_streaming("live tail one plus more text arriving");
    grown.now_ms = 200;
    let mut committed = TestRenderModel::new(width);
    committed.push_history("alpha line one");
    committed.push_history("live tail one plus more text arriving");
    committed.now_ms = 300;
    let mut scrolled = committed.clone();
    scrolled.push_history("third line settles");
    scrolled.scroll = 1;
    scrolled.now_ms = 400;
    let mut settled = scrolled.clone();
    settled.scroll = 0;
    settled.now_ms = 500;
    vec![empty, growing, grown, committed, scrolled, settled]
}

/// Wrap one plain-text line to `width` columns on grapheme-cluster
/// boundaries so ZWJ sequences and flags never split.
fn wrap_plain(text: &str, width: usize) -> Vec<String> {
    wrap_plain_with(&ColumnWidth, text, width)
}

/// Wrap one plain-text line to `width` columns measured by `measure`, so a
/// second frontend can reuse the algorithm with different font metrics.
fn wrap_plain_with(measure: &impl WidthMeasure, text: &str, width: usize) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut rows = Vec::new();
    for source in text.split('\n') {
        let mut current = String::new();
        let mut current_width = 0usize;
        for grapheme in source.graphemes(true) {
            let cw = grapheme
                .chars()
                .map(|ch| measure.cell_width(ch))
                .max()
                .unwrap_or(0);
            if current_width + cw > width && !current.is_empty() {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
            }
            current.push_str(grapheme);
            current_width += cw;
        }
        rows.push(current);
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}
