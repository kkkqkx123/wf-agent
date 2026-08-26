//! Segmented ring buffer of session output with an incremental read cursor.
//!
//! The buffer keeps the tail of the output once it exceeds
//! [`MAX_OUTPUT_BYTES`]. All positions are **absolute**: they index the
//! stream of every byte ever appended (`written`), so a byte offset recorded
//! before a truncation stays valid afterwards (a window that fell into the
//! dropped prefix is reported with a truncation marker instead of being
//! silently empty).
//!
//! The retained content is kept as a list of byte segments (each ≤
//! [`SEGMENT_BYTES`]) so appending and front-truncation are amortized O(1)
//! instead of O(n) copies on a single `String`. Every segment is valid UTF-8
//! and every segment boundary falls on a character boundary (chunks are
//! pushed whole and larger inputs are split on char boundaries), which is
//! what makes the `head_off` alignment well-defined: a partial front drop
//! only ever happens inside the head segment.

use std::collections::VecDeque;

const MAX_OUTPUT_BYTES: usize = 256_000;
/// Per-segment capacity of the segmented [`OutputBuffer`]. Segments are cut on
/// char boundaries, so a segment holds whole characters only; a production
/// chunk (≤ 4096 + 3 carry bytes from [`crate::utf8::Utf8ChunkDecoder`]) always
/// fits in a single segment, keeping the retained segment count ≤
/// `MAX_OUTPUT_BYTES / SEGMENT_BYTES`.
const SEGMENT_BYTES: usize = 8192;

/// Ring buffer of session output with an incremental read cursor.
#[derive(Default)]
pub(crate) struct OutputBuffer {
    /// Retained real output (no truncation marker). The live stream is the
    /// concatenation of `segments[0][head_off..]` and `segments[1..]`; its
    /// first byte corresponds to the absolute position `trimmed`.
    segments: VecDeque<Vec<u8>>,
    /// Bytes logically discarded inside the head segment (char-aligned; the
    /// head segment's raw bytes are kept whole, only reads skip them).
    head_off: usize,
    /// Total bytes ever appended (monotonic).
    written: usize,
    /// Bytes dropped from the front by truncation; origin of the live stream.
    trimmed: usize,
    /// Whether output was dropped; a marker is prepended on reads/snapshots.
    truncated: bool,
    /// Absolute position of the next unread output chunk.
    last_read: usize,
}

impl OutputBuffer {
    /// Bytes currently retained (`written - trimmed`).
    pub(crate) fn live_len(&self) -> usize {
        self.written - self.trimmed
    }

    pub(crate) fn append(&mut self, chunk: &str) {
        if self.live_len() + chunk.len() > MAX_OUTPUT_BYTES {
            let keep = MAX_OUTPUT_BYTES.saturating_sub(chunk.len() + 64);
            if self.live_len() > keep {
                // Drop the oldest bytes from the front. Whole segments are
                // popped in O(1); only the head segment needs a char-aligned
                // partial cut (via `head_off`), so a multi-byte UTF-8 sequence
                // is never split. A reader whose cursor fell into the dropped
                // prefix sees the truncation marker plus the retained content
                // on its next read (see `read_new`/`peek_new`).
                self.truncate_to_keep(keep);
            }
        }
        self.push_bytes(chunk.as_bytes());
        self.written += chunk.len();
    }

    /// Append `bytes` (valid UTF-8) as one or more segments, cutting on char
    /// boundaries so every segment stays valid UTF-8. Keeps the char-aligned
    /// segment-boundary invariant that `head_off` relies on. A production
    /// chunk (≤ 4096 + 3 carry bytes) fits in a single step.
    fn push_bytes(&mut self, bytes: &[u8]) {
        let mut offset = 0;
        while offset < bytes.len() {
            // `offset` is always a char boundary, so the remainder is valid
            // UTF-8 and `floor_char_boundary` is well-defined on it.
            let rest = &bytes[offset..];
            // Try to extend the existing tail segment first.
            let mut extended = false;
            if let Some(tail) = self.segments.back_mut() {
                if tail.len() < SEGMENT_BYTES {
                    let room = SEGMENT_BYTES - tail.len();
                    let want = room.min(rest.len());
                    let boundary = std::str::from_utf8(rest)
                        .expect("chunk is valid UTF-8")
                        .floor_char_boundary(want);
                    if boundary > 0 {
                        tail.extend_from_slice(&rest[..boundary]);
                        offset += boundary;
                        extended = true;
                    }
                }
            }
            if extended {
                continue;
            }
            // Tail is full or cannot fit a whole character: open a new
            // segment. `boundary` is at least 1 (the cut is ≤ 8192 bytes, or
            // the whole remainder which is a char boundary), so the loop
            // always makes progress.
            let take = SEGMENT_BYTES.min(rest.len());
            let boundary = std::str::from_utf8(rest)
                .expect("chunk is valid UTF-8")
                .floor_char_boundary(take);
            let mut seg = Vec::with_capacity(boundary);
            seg.extend_from_slice(&rest[..boundary]);
            self.segments.push_back(seg);
            offset += boundary;
        }
    }

    /// Drop bytes from the front until at most `keep` bytes are retained.
    /// Whole segments are popped in O(1); only the head segment needs a
    /// char-aligned partial cut via `head_off`.
    fn truncate_to_keep(&mut self, keep: usize) {
        let mut drop = self.live_len().saturating_sub(keep);
        while drop > 0 {
            let Some(first) = self.segments.front() else {
                break;
            };
            let avail = first.len() - self.head_off;
            if avail <= drop {
                drop -= avail;
                self.trimmed += avail;
                self.segments.pop_front();
                self.head_off = 0;
                if self.segments.is_empty() {
                    break;
                }
            } else {
                let raw = self.head_off + drop;
                let bound = std::str::from_utf8(first)
                    .expect("segment is valid UTF-8")
                    .floor_char_boundary(raw);
                self.trimmed += bound - self.head_off;
                self.head_off = bound;
                drop = 0;
            }
        }
        self.truncated = true;
    }

    pub(crate) fn snapshot(&self) -> String {
        format!("{}{}", self.marker(), self.live_text())
    }

    /// Absolute position of the next byte to be appended (callers record it to
    /// delimit the output produced by a single command).
    pub(crate) fn written(&self) -> usize {
        self.written
    }

    /// Truncation marker prepended when some prefix was dropped.
    fn marker(&self) -> String {
        if self.truncated {
            format!("(output truncated, {} bytes omitted)\n", self.trimmed)
        } else {
            String::new()
        }
    }

    /// Concatenate the retained live stream (no marker) in a single copy.
    fn live_text(&self) -> String {
        self.collect_from(0, self.live_len())
    }

    /// Copy `len` live bytes starting at live-stream offset `rel` (0 = first
    /// retained byte) into a new `String`. Segments are scanned linearly
    /// (≤ `MAX_OUTPUT_BYTES / SEGMENT_BYTES` of them); each contributes one
    /// `push_str`, so the whole read is a single copy O(n).
    fn collect_from(&self, rel: usize, len: usize) -> String {
        let mut out = String::with_capacity(len);
        let mut skip = self.head_off + rel;
        let mut remaining = len;
        for seg in self.segments.iter() {
            if remaining == 0 {
                break;
            }
            if skip >= seg.len() {
                skip -= seg.len();
                continue;
            }
            let take = (seg.len() - skip).min(remaining);
            out.push_str(
                std::str::from_utf8(&seg[skip..skip + take]).expect("segment is valid UTF-8"),
            );
            skip = 0;
            remaining -= take;
        }
        out
    }

    /// Tail of the buffer starting at the absolute position `start`. A start
    /// that fell into the dropped prefix yields the marker plus the whole
    /// retained content; a start at/beyond the end yields the empty string.
    pub(crate) fn tail_from(&self, start: usize) -> String {
        if start >= self.written {
            return String::new();
        }
        if start < self.trimmed {
            return format!("{}{}", self.marker(), self.live_text());
        }
        self.collect_from(start - self.trimmed, self.written - start)
    }

    /// Return output since the last call and advance the cursor.
    pub(crate) fn read_new(&mut self) -> String {
        if self.last_read >= self.written {
            return String::new();
        }
        if self.last_read < self.trimmed {
            let new = format!("{}{}", self.marker(), self.live_text());
            self.last_read = self.written;
            return new;
        }
        let new = self.collect_from(self.last_read - self.trimmed, self.written - self.last_read);
        self.last_read = self.written;
        new
    }

    /// Output since the cursor, without advancing it.
    pub(crate) fn peek_new(&self) -> String {
        if self.last_read >= self.written {
            return String::new();
        }
        if self.last_read < self.trimmed {
            return format!("{}{}", self.marker(), self.live_text());
        }
        self.collect_from(self.last_read - self.trimmed, self.written - self.last_read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_buffer_incremental() {
        let mut buf = OutputBuffer::default();
        buf.append("hello ");
        assert_eq!(buf.read_new(), "hello ");
        assert_eq!(buf.read_new(), "");
        buf.append("world");
        assert_eq!(buf.read_new(), "world");
        assert_eq!(buf.snapshot(), "hello world");
    }

    #[test]
    fn test_output_buffer_peek() {
        let mut buf = OutputBuffer::default();
        buf.append("line1\n");
        assert_eq!(buf.peek_new(), "line1\n");
        // Peek does not advance the cursor.
        assert_eq!(buf.peek_new(), "line1\n");
        assert_eq!(buf.read_new(), "line1\n");
        assert_eq!(buf.peek_new(), "");
    }

    #[test]
    fn test_output_buffer_truncation_resets_cursor() {
        let mut buf = OutputBuffer::default();
        let big = "x".repeat(MAX_OUTPUT_BYTES);
        buf.append(&big);
        buf.append("tail");
        let text = buf.snapshot();
        assert!(text.contains("truncated"));
        assert!(text.ends_with("tail"));
        // After truncation the cursor points at the start, so a reader sees
        // the (truncated) current content.
        assert_eq!(buf.read_new(), text);
    }

    #[test]
    fn test_output_buffer_tail_from() {
        let mut buf = OutputBuffer::default();
        buf.append("abc");
        assert_eq!(buf.tail_from(1), "bc");
        assert_eq!(buf.tail_from(3), "");
        assert_eq!(buf.tail_from(99), "");
    }

    #[test]
    fn test_output_buffer_tail_after_truncation() {
        let mut buf = OutputBuffer::default();
        // Absolute start recorded before the buffer fills up.
        let start = buf.written();
        buf.append(&"x".repeat(MAX_OUTPUT_BYTES));
        buf.append("tail");
        // A start that fell into the dropped prefix yields the marker plus the
        // retained content instead of an empty string.
        let tail = buf.tail_from(start);
        assert!(tail.contains("truncated"), "tail: {}", tail);
        assert!(tail.ends_with("tail"), "tail: {}", tail);
        // A start inside the retained region returns the plain tail.
        let later = buf.written() - "tail".len();
        assert_eq!(buf.tail_from(later), "tail");
    }

    #[test]
    fn test_output_buffer_truncation_char_boundary() {
        // Truncation must never split a multi-byte UTF-8 sequence.
        let mut buf = OutputBuffer::default();
        buf.append(&"界".repeat(MAX_OUTPUT_BYTES / 3 + 100));
        buf.append("end");
        let text = buf.snapshot();
        assert!(text.contains("truncated"));
        assert!(text.ends_with("end"));
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
        // The absolute cursor still delimiters output correctly afterwards.
        let start = buf.written();
        buf.append("tail");
        assert!(buf.tail_from(start).ends_with("tail"));
    }

    #[test]
    fn test_output_buffer_cross_segment_read() {
        // Chunks larger than a single segment must read back as the in-order
        // concatenation of all live segments.
        let mut buf = OutputBuffer::default();
        let mut expected = String::new();
        for i in 0..6 {
            let chunk = format!("chunk-{:02}-", i).repeat(600); // ~5400 bytes
            expected.push_str(&chunk);
            buf.append(&chunk);
        }
        assert!(
            buf.live_len() > SEGMENT_BYTES,
            "test must actually span multiple segments"
        );
        assert_eq!(buf.read_new(), expected);
        assert_eq!(buf.read_new(), "");
        assert_eq!(buf.snapshot(), expected);
    }

    #[test]
    fn test_output_buffer_cross_segment_truncation_char_boundary() {
        // 8192 is not a multiple of 3, so "界" (3 bytes per char) would be
        // split mid-character if segments were cut at arbitrary byte offsets.
        // Truncation must never leave a partial character at the front of the
        // retained content, including when the cut lands inside a segment.
        let mut buf = OutputBuffer::default();
        buf.append(&"界".repeat(MAX_OUTPUT_BYTES / 3));
        buf.append("end");
        let text = buf.snapshot();
        assert!(text.contains("truncated"), "text: {}", text);
        assert!(text.ends_with("end"), "text: {}", text);
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
        // The retained content (after the marker) is exactly what a fresh
        // reader sees, still valid UTF-8.
        let read = buf.read_new();
        assert_eq!(read, text);
        assert!(std::str::from_utf8(read.as_bytes()).is_ok());
    }

    #[test]
    fn test_output_buffer_marker_semantics_across_segments() {
        // Marker semantics hold when the retained content spans segments.
        let mut buf = OutputBuffer::default();
        let start = buf.written();
        buf.append(&"a".repeat(MAX_OUTPUT_BYTES));
        buf.append("tail");
        let full = buf.snapshot();
        assert!(full.contains("truncated"), "full: {}", full);
        assert!(full.ends_with("tail"), "full: {}", full);
        // A fresh cursor sees the marker plus the full retained content.
        assert_eq!(buf.read_new(), full);
        // The absolute cursor recorded before the truncation points into the
        // dropped prefix and therefore reports the marker.
        let from_start = buf.tail_from(start);
        assert!(
            from_start.contains("truncated"),
            "from_start: {}",
            from_start
        );
        assert!(from_start.ends_with("tail"), "from_start: {}", from_start);
    }

    #[test]
    fn test_output_buffer_absolute_cursor_survives_cross_segment_truncation() {
        // An absolute start recorded before the buffer fills up stays valid
        // across a truncation that spans segment boundaries.
        let mut buf = OutputBuffer::default();
        let start = buf.written();
        buf.append(&"x".repeat(MAX_OUTPUT_BYTES));
        buf.append("tail");
        let tail = buf.tail_from(start);
        assert!(tail.contains("truncated"), "tail: {}", tail);
        assert!(tail.ends_with("tail"), "tail: {}", tail);
        // A start inside the retained region returns the plain tail, located
        // via the absolute cursor past the truncated prefix.
        let later = buf.written() - "tail".len();
        assert_eq!(buf.tail_from(later), "tail");
    }
}
