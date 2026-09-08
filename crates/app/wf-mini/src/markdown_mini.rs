//! Simplified streaming markdown accumulator for the mini TUI.
//!
//! This is a stripped-down version of the full `MarkdownStream` from
//! `wf-tui`, keeping only the core committed/streaming split logic
//! without the pulldown-cmark dependency. It provides:
//!
//! - `push(delta)` — append text and return the new committed/streaming split
//! - `committed_upto()` — byte offset of the settled region
//! - `range_text(from, to)` — slice of the accumulated buffer
//! - `streaming_text()` — the in-flight text (safe prefix)
//! - `finish()` — finalize and drain

/// Characters that can open a markdown construct whose partial render
/// differs from the final one (emphasis, code spans, links, etc.).
const VIEW_UNSAFE_CHARS: &[char] = &[
    '*', '`', '_', '[', ']', '<', '>', '!', '&', '\\', '|', '#', '-', '+', '~', '=',
];

/// Frame produced by a single `push` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownFrame {
    pub new_committed: String,
    pub new_streaming: String,
}

/// Append-only streaming markdown source with committed/streaming split.
pub struct MarkdownStream {
    buffer: String,
    committed_upto: usize,
    streamed_upto: usize,
    max_source_bytes: usize,
}

impl Default for MarkdownStream {
    fn default() -> Self {
        Self::new(64 * 1024)
    }
}

impl MarkdownStream {
    pub fn new(max_source_bytes: usize) -> Self {
        Self {
            buffer: String::new(),
            committed_upto: 0,
            streamed_upto: 0,
            max_source_bytes,
        }
    }

    /// Append a delta and return the frame for this push.
    pub fn push(&mut self, delta: &str) -> MarkdownFrame {
        self.buffer.push_str(delta);
        if self.buffer.len() > self.max_source_bytes {
            return self.force_truncate();
        }
        let boundary = self.boundary();
        // Newly committed: everything from the previous committed frontier
        // up to the new boundary.
        let new_committed = if boundary > self.committed_upto {
            self.buffer[self.committed_upto..boundary].to_string()
        } else {
            String::new()
        };
        // Streaming: everything after the boundary, truncated at unsafe chars.
        let raw_streaming = &self.buffer[boundary..];
        let new_streaming = match raw_streaming.find(|c: char| VIEW_UNSAFE_CHARS.contains(&c)) {
            Some(cut) => raw_streaming[..cut].to_string(),
            None => raw_streaming.to_string(),
        };
        self.committed_upto = boundary;
        self.streamed_upto = self.buffer.len();
        MarkdownFrame {
            new_committed,
            new_streaming,
        }
    }

    /// Byte offset where the committed region ends.
    pub fn committed_upto(&self) -> usize {
        self.committed_upto
    }

    /// The current in-flight (streaming) source slice, truncated at the
    /// first character that could open a markdown construct.
    pub fn streaming_text(&self) -> &str {
        let tail = &self.buffer[self.committed_upto.min(self.buffer.len())..];
        match tail.find(|c: char| VIEW_UNSAFE_CHARS.contains(&c)) {
            Some(cut) => &tail[..cut],
            None => tail,
        }
    }

    /// Source bytes in `[from, to)` of the cumulative buffer.
    pub fn range_text(&self, from: usize, to: usize) -> &str {
        let len = self.buffer.len();
        &self.buffer[from.min(len)..to.min(len)]
    }

    /// Finalize: return remaining undelivered bytes and clear.
    pub fn finish(&mut self) -> MarkdownFrame {
        let committed = self.buffer[self.committed_upto..].to_string();
        self.buffer.clear();
        self.committed_upto = 0;
        self.streamed_upto = 0;
        MarkdownFrame {
            new_committed: committed,
            new_streaming: String::new(),
        }
    }

    fn force_truncate(&mut self) -> MarkdownFrame {
        let mut end = self.max_source_bytes;
        while end > 0 && !self.buffer.is_char_boundary(end) {
            end -= 1;
        }
        let start = self.committed_upto.min(end);
        let committed = self.buffer[start..end].to_string();
        let rest: String = self.buffer[end..].to_string();
        self.buffer = rest;
        self.committed_upto = 0;
        self.streamed_upto = 0;
        MarkdownFrame {
            new_committed: committed,
            new_streaming: String::new(),
        }
    }

    /// Simplified boundary: split at the last blank line, or at the
    /// last newline when the last line looks complete.
    fn boundary(&self) -> usize {
        if self.buffer.is_empty() {
            return 0;
        }
        // Split at last blank line (double newline).
        if let Some(pos) = self.buffer.rfind("\n\n") {
            return pos + 2;
        }
        // No blank line: keep everything streaming until finalize.
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_returns_committed_and_streaming() {
        let mut md = MarkdownStream::new(1024);
        let f1 = md.push("hello ");
        assert!(f1.new_committed.is_empty());
        assert_eq!(f1.new_streaming, "hello ");

        let f2 = md.push("world\n\n");
        assert_eq!(f2.new_committed, "hello world\n\n");
        assert!(f2.new_streaming.is_empty());
    }

    #[test]
    fn finish_returns_remaining() {
        let mut md = MarkdownStream::new(1024);
        md.push("partial text");
        let f = md.finish();
        assert_eq!(f.new_committed, "partial text");
        assert!(f.new_streaming.is_empty());
    }

    #[test]
    fn truncate_at_max() {
        let mut md = MarkdownStream::new(10);
        md.push("0123456789"); // exactly at limit
        let f = md.push("extra");
        assert!(!f.new_committed.is_empty());
    }
}
