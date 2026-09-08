//! Proxy backend that records every frame's diff bytes for performance analysis.
//!
//! Wraps any [`ratatui::backend::Backend`] whose error type is [`io::Error`]
//! (e.g. [`CrosstermBackend`](ratatui::backend::CrosstermBackend)) and
//! intercepts `draw()` to capture the ANSI byte stream produced by the diff
//! engine. The captured bytes are written to an output target (file / buffer)
//! alongside per-frame metadata (frame number, byte count), while the original
//! draw is forwarded unchanged to the inner backend.
//!
//! **Gate**: only compiled with the `diff-record` feature flag.
//!
//! Run the recording example:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_diff_record --features diff-record
//! ```

use std::io::{self, Write};

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};

/// A backend decorator that taps into every `draw()` call and writes the
/// resulting ANSI bytes to a secondary output alongside the real render.
pub struct DiffRecorderBackend<B: Backend<Error = io::Error>> {
    inner: B,
    recorder: Box<dyn Write + Send>,
    frame_count: u64,
}

impl<B: Backend<Error = io::Error>> DiffRecorderBackend<B> {
    /// Wrap `inner` backend and send every frame's diff bytes to `recorder`.
    pub fn new(inner: B, recorder: impl Write + Send + 'static) -> Self {
        Self {
            inner,
            recorder: Box::new(recorder),
            frame_count: 0,
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

        // Render the diff to an in-memory buffer to obtain raw ANSI bytes.
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
}
