//! Proxy backend that records every frame's diff bytes for performance analysis.
//!
//! Wraps any [`ratatui::backend::Backend`] whose error type is [`io::Error`]
//! (e.g. [`CrosstermBackend`](ratatui::backend::CrosstermBackend)) and
//! intercepts `draw()` to capture the ANSI byte stream produced by the diff
//! engine. The captured bytes are written to an output target (file / buffer)
//! alongside per-frame metadata (frame number, byte count), while the original
//! draw is forwarded unchanged to the inner backend.
//!
//! Each frame also feeds [`FrameMetrics`](crate::frame_metrics::FrameMetrics):
//! changed cells (the diff iterator length ratatui hands over), diff bytes,
//! frame time, plus layout/parse sampling points the preparation layer
//! reports via [`DiffRecorderBackend::note_layout`] before the next draw.
//!
//! **Gate**: only compiled with the `diff-record` feature flag.
//!
//! Run the recording example:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_diff_record --features diff-record
//! ```

use std::io::{self, Write};
use std::time::Instant;

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};

use crate::frame_metrics::{FrameMetric, FrameMetrics};

/// A backend decorator that taps into every `draw()` call and writes the
/// resulting ANSI bytes to a secondary output alongside the real render.
pub struct DiffRecorderBackend<B: Backend<Error = io::Error>> {
    inner: B,
    recorder: Box<dyn Write + Send>,
    frame_count: u64,
    metrics: FrameMetrics,
    last_draw: Option<Instant>,
    pending_layout_rows: usize,
    pending_parsed_bytes: usize,
}

impl<B: Backend<Error = io::Error>> DiffRecorderBackend<B> {
    /// Wrap `inner` backend and send every frame's diff bytes to `recorder`.
    pub fn new(inner: B, recorder: impl Write + Send + 'static) -> Self {
        Self {
            inner,
            recorder: Box::new(recorder),
            frame_count: 0,
            metrics: FrameMetrics::new(),
            last_draw: None,
            pending_layout_rows: 0,
            pending_parsed_bytes: 0,
        }
    }

    /// Unwrap and return the inner backend (for terminal restoration).
    pub fn into_inner(self) -> B {
        self.inner
    }

    /// Number of frames recorded so far.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Report the layout/parse sampling point for the next drawn frame.
    /// Called by the preparation layer before the frame is submitted.
    pub fn note_layout(&mut self, laid_out_rows: usize, parsed_bytes: usize) {
        self.pending_layout_rows = laid_out_rows;
        self.pending_parsed_bytes = parsed_bytes;
    }

    /// Metrics collected across every recorded frame.
    pub fn metrics(&self) -> &FrameMetrics {
        &self.metrics
    }

    /// One-line baseline report over the recorded frames.
    pub fn metrics_report(&self) -> String {
        self.metrics.report()
    }
}

impl<B: Backend<Error = io::Error>> Backend for DiffRecorderBackend<B> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        // Collect the diff cells so we can iterate twice: once for the
        // recorder's ANSI rendering, once for the real backend.
        let cells: Vec<(u16, u16, &Cell)> = content.collect();
        let changed_cells = cells.len();

        // Render the diff to an in-memory buffer to obtain raw ANSI bytes.
        let draw_start = Instant::now();
        let mut ansi_buf: Vec<u8> = Vec::with_capacity(1024);
        {
            let mut temp = ratatui::backend::CrosstermBackend::new(&mut ansi_buf);
            temp.draw(cells.iter().cloned())?;
            Backend::flush(&mut temp)?;
        }

        // Write frame header + ANSI payload to the recorder output.
        let header = format!(
            "\n==== FRAME {} (bytes:{}) ====\n",
            self.frame_count,
            ansi_buf.len()
        );
        self.recorder.write_all(header.as_bytes())?;
        self.recorder.write_all(&ansi_buf)?;
        self.recorder.flush()?;
        self.frame_count += 1;

        let elapsed_ms = self
            .last_draw
            .map(|last| last.elapsed().as_millis() as u64)
            .unwrap_or(0);
        self.last_draw = Some(draw_start);
        self.metrics.record(FrameMetric::new(
            self.frame_count,
            changed_cells,
            ansi_buf.len(),
            elapsed_ms,
            std::mem::take(&mut self.pending_layout_rows),
            std::mem::take(&mut self.pending_parsed_bytes),
        ));

        // Forward to the real backend.
        self.inner.draw(cells.into_iter())?;

        Ok(())
    }

    fn append_lines(&mut self, n: u16) -> Result<(), Self::Error> {
        self.inner.append_lines(n)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_frame_and_delegates() {
        let real = ratatui::backend::CrosstermBackend::new(Vec::new());
        let mut rec = DiffRecorderBackend::new(real, Vec::<u8>::new());
        rec.draw(std::iter::empty()).unwrap();
        rec.flush().unwrap();
        assert_eq!(rec.frame_count(), 1);
        assert!(rec.size().is_ok());
    }

    #[test]
    fn multiple_frames_increment_count() {
        let real = ratatui::backend::CrosstermBackend::new(Vec::new());
        let mut rec = DiffRecorderBackend::new(real, Vec::<u8>::new());
        for _ in 0..5 {
            rec.draw(std::iter::empty()).unwrap();
        }
        assert_eq!(rec.frame_count(), 5);
    }

    #[test]
    fn into_inner_returns_original_backend() {
        let real = ratatui::backend::CrosstermBackend::new(Vec::new());
        let rec = DiffRecorderBackend::new(real, Vec::<u8>::new());
        let _ = rec.into_inner();
    }

    #[test]
    fn frames_feed_metrics_with_layout_sampling() {
        let real = ratatui::backend::CrosstermBackend::new(Vec::new());
        let mut rec = DiffRecorderBackend::new(real, Vec::<u8>::new());
        rec.note_layout(3, 40);
        rec.draw(std::iter::empty()).unwrap();
        rec.note_layout(1, 12);
        rec.draw(std::iter::empty()).unwrap();
        assert_eq!(rec.metrics().len(), 2);
        assert_eq!(rec.metrics().total_laid_out_rows(), 4);
        assert_eq!(rec.metrics().total_parsed_bytes(), 52);
        assert!(rec.metrics_report().contains("frames=2"));
    }
}
