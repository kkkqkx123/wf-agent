//! Diagnostics channel for headless sessions (stderr).

use std::io::{self, Write};

// ── diagnostics channel (stderr) ─────────────────────────────────────

const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";

/// Diagnostics writer for stderr-bound output (tool lines, rejections,
/// interrupt notices). Every line is captured in an in-memory buffer
/// (snapshot for tests / summaries) and optionally mirrored to stderr for
/// real runs; shared with the approval/interaction callbacks via
/// `Arc<Mutex<...>>`.
pub struct DiagWriter {
    captured: Vec<u8>,
    mirror: Option<Box<dyn Write + Send>>,
    color: bool,
}

impl DiagWriter {
    /// Bound to process stderr; `color` follows the TTY/no-color answer.
    pub fn stderr(color: bool) -> Self {
        Self {
            captured: Vec::new(),
            mirror: Some(Box::new(io::stderr())),
            color,
        }
    }

    /// Bound to an in-memory buffer only (tests).
    pub fn buffer() -> Self {
        Self {
            captured: Vec::new(),
            mirror: None,
            color: false,
        }
    }

    /// Append one diagnostic line (newline terminated, flushed on the
    /// mirror when present).
    pub fn line(&mut self, text: &str) -> io::Result<()> {
        self.captured.extend_from_slice(text.as_bytes());
        self.captured.push(b'\n');
        if let Some(mirror) = self.mirror.as_mut() {
            mirror.write_all(text.as_bytes())?;
            mirror.write_all(b"\n")?;
            mirror.flush()?;
        }
        Ok(())
    }

    /// Accumulated diagnostics so far (lossy UTF-8).
    pub fn snapshot(&self) -> String {
        String::from_utf8_lossy(&self.captured).into_owned()
    }

    pub(crate) fn ok(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            self.line(&format!("{GREEN}{text}{RESET}"))
        } else {
            self.line(text)
        }
    }

    pub(crate) fn err(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            self.line(&format!("{RED}{text}{RESET}"))
        } else {
            self.line(text)
        }
    }

    pub(crate) fn warn(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            self.line(&format!("{YELLOW}{text}{RESET}"))
        } else {
            self.line(text)
        }
    }
}
