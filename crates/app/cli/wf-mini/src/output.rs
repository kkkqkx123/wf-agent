//! Append-only stdout writer: LLM deltas stream verbatim, flushed on
//! newline boundaries so partial lines are not stranded mid-row.

use std::io::{self, Write};

/// Flush threshold for a single unterminated line.
const MAX_LINE_BYTES: usize = 8 * 1024;

/// Buffers streaming text and releases it in newline-terminated chunks.
#[derive(Debug, Default)]
pub struct AppendWriter {
    buf: String,
}

impl AppendWriter {
    pub fn new() -> Self {
        Self { buf: String::new() }
    }

    /// Append a delta, returning the segment that is ready to write.
    pub fn push(&mut self, delta: &str) -> String {
        self.buf.push_str(delta);
        if self.buf.contains('\n') {
            let split = self.buf.rfind('\n').expect("checked above") + 1;
            let ready = self.buf[..split].to_string();
            self.buf.replace_range(..split, "");
            ready
        } else if self.buf.len() >= MAX_LINE_BYTES {
            std::mem::take(&mut self.buf)
        } else {
            String::new()
        }
    }

    /// Release whatever is buffered, even without a trailing newline.
    pub fn take_remaining(&mut self) -> String {
        std::mem::take(&mut self.buf)
    }
}

/// Write raw text to stdout and flush. stdout carries only business output
/// (assistant text), so piped copies stay clean.
pub fn write_stdout(text: &str) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    out.write_all(text.as_bytes())?;
    out.flush()
}

/// Write raw text to stderr and flush. Used for partial diagnostic streams
/// (reasoning deltas) that must not wait for a line boundary.
pub fn write_stderr(text: &str) -> io::Result<()> {
    let stderr = io::stderr();
    let mut err = stderr.lock();
    err.write_all(text.as_bytes())?;
    err.flush()
}

/// Write one diagnostic line to stderr. Tool lifecycle, summaries, approval
/// notes and errors all go here, never to stdout.
pub fn diag_line(text: &str) {
    eprintln!("{text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_complete_lines_only() {
        let mut writer = AppendWriter::new();
        assert_eq!(writer.push("hello "), "");
        assert_eq!(writer.push("world\ntail"), "hello world\n");
        assert_eq!(writer.take_remaining(), "tail");
    }

    #[test]
    fn flushes_long_lines_without_newline() {
        let mut writer = AppendWriter::new();
        let chunk = "x".repeat(MAX_LINE_BYTES);
        assert_eq!(writer.push(&chunk), chunk);
    }

    #[test]
    fn take_remaining_drains() {
        let mut writer = AppendWriter::new();
        assert_eq!(writer.push("partial"), "");
        assert_eq!(writer.take_remaining(), "partial");
        assert_eq!(writer.take_remaining(), "");
    }
}
