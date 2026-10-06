//! Panic fallback drawing and isolated component draw.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::frame_metrics::FrameMetric;

/// Render the panic fallback into `buf`: a recognizable frame proving the
/// loop survived a failing widget.
pub fn draw_recovered_frame(buf: &mut Buffer, area: Rect) {
    let msg = "render recovered after panic";
    buf.set_string(area.x, area.y, msg, Style::default());
}

/// Draw one component in isolation: a panic degrades to the recovered
/// placeholder for that area and records a metric instead of killing the
/// frame. Returns the elapsed metric for budget accounting.
pub fn isolated_component_draw(
    frame_no: u64,
    buf: &mut Buffer,
    area: Rect,
    draw: impl FnOnce(&mut Buffer, Rect),
) -> FrameMetric {
    let start = std::time::Instant::now();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        draw(buf, area);
    }));
    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    if outcome.is_err() {
        draw_recovered_frame(buf, area);
        tracing::warn!("tui: isolated component panicked, recovered placeholder drawn");
    }
    FrameMetric::new(
        frame_no,
        area.width as usize * area.height as usize,
        area.width as usize * area.height as usize * 8,
        elapsed_ms,
        0,
        0,
    )
}
