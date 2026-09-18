//! Incremental streaming Markdown rendering.
//!
//! [`MarkdownStream`] holds the cumulative source buffer and re-parses it on
//! every `push` with pulldown-cmark's pull parser (`into_offset_iter`),
//! splitting the source into:
//!
//! * **committed** — the settled top-level blocks (everything before the
//!   last in-flight block) plus whatever the settlement heuristic closed;
//! * **streaming** — the last in-flight block, shown only once its content
//!   lines are complete (unfinished blocks are not frozen).
//!
//! The split is delivered incrementally as `new_committed` / `new_streaming`
//! so consumers never re-emit the same source bytes. `code_lang` exposes the
//! language of an in-flight fenced code block for later syntax highlighting.
//! Over-limit source is force-truncated and force-committed so a long output
//! never drags down per-frame re-parsing.

use super::blocks::{
    extract_code_lang, fence_open, has_reference_definition, last_top_level_block_start,
    settle_boundary, unclosed_table_start,
};
use super::plain::render_plain_text;

/// Default source cap for the cumulative buffer (64 KiB).
pub const DEFAULT_MAX_SOURCE_BYTES: usize = 64 * 1024;

/// Default interval between throttled parses (about one frame at 60fps).
pub const DEFAULT_PARSE_INTERVAL_MS: u64 = 16;

/// Characters that can open (or sit inside) a markdown construct whose
/// partial parse renders differently from the final one: emphasis `* _`,
/// code spans/backticks, links/images/refs `[ ] !`, autolinks/entities
/// `< > &`, escapes `\`, tables `|`, headings `#`, list bullets `- + ~`
/// and setext/rules `= - ~`. The streaming view is truncated at the first
/// occurrence (see [`MarkdownStream::streaming_text`]) so the visible
/// text can never run ahead of the final plain render.
const VIEW_UNSAFE_CHARS: &[char] = &[
    '*', '`', '_', '[', ']', '<', '>', '!', '&', '\\', '|', '#', '-', '+', '~', '=',
];

/// A frame of streaming markdown output for one `push`/`finish` call.
///
/// Both text fields are **source text** (never width-fixed render caches);
/// reflow and styling belong to `HistoryLine` and the interactive renderers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownFrame {
    /// Newly settled source text since the previous call (never re-emitted).
    pub new_committed: String,
    /// Newly visible in-flight source text since the previous call.
    pub new_streaming: String,
    /// Language tag of the in-flight fenced code block, if any.
    pub code_lang: Option<String>,
}

/// Parse cost accounting for throttling regression asserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParseMetrics {
    /// Bytes fed to the pulldown parser on the last parse.
    pub last_bytes: usize,
    /// Total parser input bytes across all throttled parses.
    pub total_bytes: u64,
    /// Full-buffer parses (fallback or cold start).
    pub full_parses: u64,
    /// Tail-only incremental parses.
    pub incremental_parses: u64,
}

/// Append-only streaming markdown source with committed/streaming split.
pub struct MarkdownStream {
    buffer: String,
    /// Source bytes already delivered as committed text.
    committed_upto: usize,
    /// Source bytes already delivered as streaming text.
    streamed_upto: usize,
    max_source_bytes: usize,
    /// True when unparsed deltas arrived since the last throttled parse.
    dirty: bool,
    /// Timestamp of the last throttled parse in caller milliseconds.
    last_parse_ms: Option<u64>,
    /// Minimum gap between throttled parses.
    parse_interval_ms: u64,
    /// Committed frontier reused as the immutable safe prefix.
    cached_safe_len: usize,
    metrics: ParseMetrics,
}

impl Default for MarkdownStream {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SOURCE_BYTES)
    }
}

impl MarkdownStream {
    /// New stream with an explicit source cap.
    pub fn new(max_source_bytes: usize) -> Self {
        Self {
            buffer: String::new(),
            committed_upto: 0,
            streamed_upto: 0,
            max_source_bytes,
            dirty: false,
            last_parse_ms: None,
            parse_interval_ms: DEFAULT_PARSE_INTERVAL_MS,
            cached_safe_len: 0,
            metrics: ParseMetrics::default(),
        }
    }

    /// New stream with an explicit parse throttle interval.
    pub fn new_with_interval(max_source_bytes: usize, parse_interval_ms: u64) -> Self {
        let mut stream = Self::new(max_source_bytes);
        stream.parse_interval_ms = parse_interval_ms;
        stream
    }

    /// Minimum gap between throttled parses in milliseconds.
    pub fn parse_interval_ms(&self) -> u64 {
        self.parse_interval_ms
    }

    /// Override the throttle interval (tests use zero for immediacy).
    pub fn set_parse_interval_ms(&mut self, interval_ms: u64) {
        self.parse_interval_ms = interval_ms;
    }

    /// True when unparsed deltas are waiting for the frame preparation stage.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Parse cost accounting for frame-budget asserts.
    pub fn parse_metrics(&self) -> ParseMetrics {
        self.metrics
    }

    /// Throttled arrival: append the delta and mark dirty without parsing.
    /// Over-limit input is committed synchronously and reported immediately
    /// so a long output never waits on the throttle interval.
    pub fn push_throttled(&mut self, delta: &str) -> Option<MarkdownFrame> {
        if delta.is_empty() {
            return None;
        }
        self.buffer.push_str(delta);
        if self.buffer.len() > self.max_source_bytes {
            return Some(self.force_truncate());
        }
        self.dirty = true;
        None
    }

    /// Frame preparation stage: the only throttled parse trigger. Returns a
    /// frame when dirty and due (or when forced), otherwise `None` so one
    /// frame coalesces many arrivals.
    pub fn prepare_frame(&mut self, now_ms: u64, force: bool) -> Option<MarkdownFrame> {
        if !self.dirty {
            return None;
        }
        if !force {
            if let Some(last) = self.last_parse_ms {
                if now_ms.saturating_sub(last) < self.parse_interval_ms {
                    return None;
                }
            }
        }
        let boundary = self.boundary_incremental();
        self.last_parse_ms = Some(now_ms);
        self.dirty = false;
        Some(self.frame_for_boundary(boundary))
    }

    /// Append a delta and return the frame for this push.
    ///
    /// Reference implementation: parses immediately on every call. The
    /// interactive path uses `push_throttled` plus `prepare_frame` instead so
    /// bursts coalesce into one parse per frame; this method stays as the
    /// correctness backstop and for the existing equivalence tests.
    pub fn push(&mut self, delta: &str) -> MarkdownFrame {
        self.buffer.push_str(delta);
        if self.buffer.len() > self.max_source_bytes {
            return self.force_truncate();
        }
        let boundary = self.boundary();
        self.cached_safe_len = self.committed_upto.max(boundary).min(self.buffer.len());
        self.frame_for_boundary(boundary)
    }

    /// Full accumulated source buffer (for whole-buffer renderers such as
    /// the headless renderer, which re-render and diff instead of consuming
    /// the committed/streaming split).
    pub fn source(&self) -> &str {
        &self.buffer
    }

    /// Byte offset where the committed (settled) region ends and the
    /// in-flight block begins.
    pub fn committed_upto(&self) -> usize {
        self.committed_upto
    }

    /// True when the in-flight (streaming) block is an unclosed fenced code
    /// block — its streaming lines are raw code content, not rendered text.
    pub fn streaming_is_code(&self) -> bool {
        let start = self.committed_upto.min(self.buffer.len());
        fence_open(&self.buffer[start..])
    }

    /// The current in-flight (streaming) source slice: everything after the
    /// committed boundary, truncated at the first character that could open
    /// a markdown construct whose partial parse differs from the final
    /// render (emphasis, code spans, links, tables, headings, lists…).
    /// The visible streaming text is therefore always a prefix of the
    /// final plain render — the streaming view never runs ahead of what
    /// the settled document will show.
    pub fn streaming_text(&self) -> &str {
        let tail = &self.buffer[self.committed_upto.min(self.buffer.len())..];
        match tail.find(|c: char| VIEW_UNSAFE_CHARS.contains(&c)) {
            Some(cut) => &tail[..cut],
            None => tail,
        }
    }

    /// Source bytes in `[from, to)` of the cumulative buffer (`to` clamped
    /// to the end). Consumers that track their own settlement frontier
    /// (the TUI inline scrollback cover) use this to flush the exact remaining
    /// span at a finalize boundary.
    pub fn range_text(&self, from: usize, to: usize) -> &str {
        let len = self.buffer.len();
        &self.buffer[from.min(len)..to.min(len)]
    }

    /// Finalize-time safety net: render the full
    /// accumulated source to plain text. Consumers use this as the
    /// correctness backstop when a finalize lands after in-stream resizes.
    /// The committed/streaming split is the fast path; this is the
    /// whole-source ground truth. Must be called before `finish` (which
    /// drains the buffer).
    pub fn final_plain_text(&self) -> String {
        render_plain_text(&self.buffer)
    }

    /// Close the stream. Only bytes never delivered in any earlier frame are
    /// returned — previously streamed bytes belong to the consumer's
    /// streaming view, which the consumer settles itself (the inline form flushes its
    /// scrollback span via [`Self::range_text`] before finishing). Never
    /// re-emits, never drops.
    pub fn finish(&mut self) -> MarkdownFrame {
        let committed = self.buffer[self.streamed_upto.min(self.buffer.len())..].to_string();
        self.buffer.clear();
        self.committed_upto = 0;
        self.streamed_upto = 0;
        self.dirty = false;
        self.last_parse_ms = None;
        self.cached_safe_len = 0;
        MarkdownFrame {
            new_committed: committed,
            new_streaming: String::new(),
            code_lang: None,
        }
    }

    /// Over-limit protection: cut at a char boundary near the cap, commit
    /// everything that was not yet delivered and keep the tail for the next
    /// push. Also marks the stream clean so a caller's over-limit branch
    /// returns straight away. Never panics on non-boundary splits.
    fn force_truncate(&mut self) -> MarkdownFrame {
        let src = &self.buffer;
        let mut end = self.max_source_bytes;
        while end > 0 && !src.is_char_boundary(end) {
            end -= 1;
        }
        let start = self.committed_upto.min(end);
        let committed = src[start..end].to_string();
        let rest: String = src[end..].to_string();
        self.buffer = rest;
        self.committed_upto = 0;
        self.streamed_upto = 0;
        self.cached_safe_len = 0;
        self.dirty = false;
        MarkdownFrame {
            new_committed: committed,
            new_streaming: String::new(),
            code_lang: None,
        }
    }

    /// Build the incremental frame for a settled boundary and advance the
    /// delivery frontiers. Shared by the immediate and throttled paths so
    /// both offer identical committed/streaming semantics.
    fn frame_for_boundary(&mut self, boundary: usize) -> MarkdownFrame {
        let new_committed = self.buffer[self.streamed_upto.min(boundary)..boundary].to_string();
        let stream_start = self.streamed_upto.max(boundary);
        let new_streaming = self.buffer[stream_start..].to_string();
        let code_lang = if boundary >= self.buffer.len() {
            None
        } else {
            extract_code_lang(&self.buffer[boundary..])
        };
        self.committed_upto = self.committed_upto.max(boundary);
        self.streamed_upto = self.buffer.len();
        self.cached_safe_len = self.committed_upto.min(self.buffer.len());
        MarkdownFrame {
            new_committed,
            new_streaming,
            code_lang,
        }
    }

    /// Record a full-buffer parse in the metrics.
    fn count_full_parse(&mut self) {
        let n = self.buffer.len();
        self.metrics.last_bytes = n;
        self.metrics.total_bytes += n as u64;
        self.metrics.full_parses += 1;
    }

    /// Incremental boundary: reuse the settled prefix and only reparse the
    /// tail. Suspicious structures (reference definitions, unclosed tables or
    /// fences) fall back to a full parse under the existing holdback rules.
    fn boundary_incremental(&mut self) -> usize {
        if self.buffer.is_empty() {
            self.metrics.last_bytes = 0;
            return 0;
        }
        if has_reference_definition(&self.buffer) || self.buffer.contains("][") {
            self.count_full_parse();
            return self.boundary();
        }
        if fence_open(&self.buffer) {
            self.count_full_parse();
            return self.boundary();
        }
        if let Some(start) = unclosed_table_start(&self.buffer) {
            self.count_full_parse();
            return start.min(self.buffer.len());
        }
        let safe = self.cached_safe_len.min(self.buffer.len());
        if safe == 0 || !self.buffer.is_char_boundary(safe) {
            self.count_full_parse();
            return self.boundary();
        }
        let tail = &self.buffer[safe..];
        self.metrics.last_bytes = tail.len();
        self.metrics.total_bytes += tail.len() as u64;
        self.metrics.incremental_parses += 1;
        let split = last_top_level_block_start(tail)
            .map(|offset| offset + safe)
            .unwrap_or(self.buffer.len());
        let boundary = settle_boundary(&self.buffer, split);
        boundary.max(safe.min(boundary))
    }

    /// Byte offset that splits committed from streaming for the current
    /// buffer: the start of the last top-level block, or the buffer end when
    /// the settlement heuristic closed that block.
    ///
    /// Three correctness gates apply on top of the block heuristic:
    ///
    /// * **reference-definition fallback** — when the buffer carries a
    ///   reference-style link definition (`[label]: url`) or a potential
    ///   reference usage (`…][…`), incremental splitting is skipped and
    ///   the whole source stays streaming until finalize (a later
    ///   definition rewrites earlier link targets);
    /// * **table holdback** — an unclosed table (header + delimiter row
    ///   present, rows still continuing) keeps the whole table streaming
    ///   until finalize so column widths never shift mid-stream;
    /// * **newline gate** — the commit point never passes the last newline,
    ///   so a half line is never committed (see [`settle_boundary`]).
    fn boundary(&self) -> usize {
        if self.buffer.is_empty() {
            return 0;
        }
        if has_reference_definition(&self.buffer) || self.buffer.contains("][") {
            return 0;
        }
        if !fence_open(&self.buffer) {
            if let Some(start) = unclosed_table_start(&self.buffer) {
                return start;
            }
        }
        let split = last_top_level_block_start(&self.buffer).unwrap_or(self.buffer.len());
        settle_boundary(&self.buffer, split)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::render_plain_text;

    #[test]
    fn unfinished_paragraph_stays_streaming() {
        let mut stream = MarkdownStream::default();
        let frame = stream.push("hello wo");
        assert_eq!(frame.new_committed, "");
        assert_eq!(frame.new_streaming, "hello wo");
        assert_eq!(frame.code_lang, None);
    }

    #[test]
    fn blank_line_settles_the_paragraph() {
        let mut stream = MarkdownStream::default();
        stream.push("hello world");
        let frame = stream.push("\n\n");
        assert_eq!(frame.new_committed, "\n\n");
        assert_eq!(frame.new_streaming, "");
        assert_eq!(stream.finish().new_committed, "");
    }

    #[test]
    fn paragraph_then_new_paragraph_only_commits_the_first() {
        let mut stream = MarkdownStream::default();
        stream.push("first para");
        let frame = stream.push("\n\nsecond");
        // First block settled by the blank line; the second stays in flight.
        assert_eq!(frame.new_committed, "\n\n");
        assert_eq!(frame.new_streaming, "second");
    }

    #[test]
    fn list_blocks_stream_and_settle_on_blank_line() {
        let mut stream = MarkdownStream::default();
        let frame = stream.push("- item one\n");
        assert_eq!(frame.new_committed, "");
        assert_eq!(frame.new_streaming, "- item one\n");
        let frame = stream.push("- item two");
        assert_eq!(frame.new_streaming, "- item two");
        let frame = stream.push("\n\n");
        assert_eq!(frame.new_committed, "\n\n");
        assert_eq!(frame.new_streaming, "");
    }

    #[test]
    fn heading_streams_and_settles() {
        let mut stream = MarkdownStream::default();
        stream.push("# Title");
        // Nothing new delivered by an empty push (incremental delivery).
        assert_eq!(stream.push("").new_streaming, "");
        let frame = stream.push("\n\n");
        assert_eq!(frame.new_committed, "\n\n");
    }

    #[test]
    fn unclosed_fence_streams_line_by_line_then_settles() {
        let mut stream = MarkdownStream::default();
        let frame = stream.push("```rust\nfn main() {\n");
        assert_eq!(frame.new_committed, "");
        assert_eq!(frame.new_streaming, "```rust\nfn main() {\n");
        assert_eq!(frame.code_lang.as_deref(), Some("rust"));

        // More code keeps streaming (no blank-line or closing fence).
        let frame = stream.push("    println!();\n");
        assert_eq!(frame.new_committed, "");
        assert!(frame.new_streaming.contains("println"));

        // The closing fence settles the block, but the gate keeps the
        // trailing half line (no newline after the closing fence) streaming.
        let frame = stream.push("}\n```");
        assert_eq!(frame.new_committed, "}\n");
        assert_eq!(frame.new_streaming, "```");
        assert_eq!(frame.code_lang, None);
        // Finalize delivers only undelivered bytes: the "```" half line was
        // already streamed above (never re-emitted).
        assert_eq!(stream.finish().new_committed, "");
    }

    #[test]
    fn blank_line_inside_code_block_does_not_settle() {
        let mut stream = MarkdownStream::default();
        stream.push("```rust\nfn main() {\n");
        let frame = stream.push("\n");
        // An empty line inside the fenced block must not freeze the block;
        // the only new delivery is the blank line itself (incremental).
        assert_eq!(frame.new_committed, "");
        assert_eq!(frame.new_streaming, "\n");
    }

    #[test]
    fn finish_commits_everything_remaining() {
        let mut stream = MarkdownStream::default();
        stream.push("unfinished");
        // The streamed tail was already delivered, so the finalize frame
        // carries nothing new — the consumer settles its own streaming view
        // (see `range_text`).
        let frame = stream.finish();
        assert_eq!(frame.new_committed, "");
        assert_eq!(frame.new_streaming, "");
        // The buffer is drained: subsequent pushes start fresh.
        assert_eq!(stream.push("next").new_streaming, "next");
    }

    #[test]
    fn over_limit_source_is_force_committed_without_panicking() {
        // Cap at 16 bytes; a delta of repeated CJK text must be truncated at
        // a char boundary and committed without a panic, with the tail kept
        // for the next push.
        let mut stream = MarkdownStream::new(16);
        let frame = stream.push("横横横横横横横横横横横横横横");
        assert!(!frame.new_committed.is_empty());
        assert_eq!(frame.new_streaming, "");
        // The tail is preserved and the '后' byte is never split across a
        // char boundary on the next push.
        let frame = stream.push("后");
        assert_eq!(frame.new_committed, "横横横横横");
        assert_eq!(frame.new_streaming, "");
        assert_eq!(stream.source(), "横横横横后");
    }

    #[test]
    fn incremental_delivery_never_reemits_source_bytes() {
        let mut stream = MarkdownStream::default();
        let mut seen = String::new();
        for chunk in ["hello ", "world\n\n", "```rust\n", "code\n", "```"] {
            let frame = stream.push(chunk);
            seen.push_str(&frame.new_committed);
            seen.push_str(&frame.new_streaming);
        }
        assert_eq!(seen, "hello world\n\n```rust\ncode\n```");
    }

    #[test]
    fn reference_definition_keeps_entire_buffer_streaming() {
        let mut stream = MarkdownStream::default();
        stream.push("click [here][link] for more.\n");
        let frame = stream.push("\n[link]: https://example.com\n");
        assert_eq!(frame.new_committed, "");
        assert!(frame.new_streaming.contains("[link]: https://example.com"));
        assert_eq!(stream.finish().new_committed, stream.source());
    }

    #[test]
    fn unclosed_table_holds_back_until_finalize() {
        let mut stream = MarkdownStream::default();
        let hdr = "| Name | Value |\n| --- | --- |\n";
        let row1 = "| foo | bar |\n";
        let row2 = "| baz | qux |\n";
        stream.push(hdr);
        stream.push(row1);
        let frame = stream.push(row2);
        // Holdback: no table byte is ever committed while the table may
        // still grow rows.
        assert_eq!(frame.new_committed, "");
        // The rows still flow as streaming deltas (live preview).
        assert!(frame.new_streaming.contains(row2));
        // Reassembly across frames: committed + streaming deltas carry the
        // whole table exactly once, and finalize adds nothing (the consumer
        // settles its own streaming view).
        let mut delivered = String::new();
        let mut replay = MarkdownStream::default();
        for chunk in [hdr, row1, row2] {
            let f = replay.push(chunk);
            delivered.push_str(&f.new_committed);
            delivered.push_str(&f.new_streaming);
        }
        assert_eq!(delivered, format!("{hdr}{row1}{row2}"));
        assert_eq!(replay.finish().new_committed, "");
    }

    #[test]
    fn newline_gate_never_commits_a_half_line() {
        let mut stream = MarkdownStream::default();
        let f1 = stream.push("first line\nsecond half");
        assert_eq!(f1.new_committed, "");
        assert_eq!(f1.new_streaming, "first line\nsecond half");
        let f2 = stream.push(" done\n");
        // The paragraph is still in flight (no blank line yet): only the
        // new bytes stream, and nothing that does not end at a line break
        // is ever committed.
        assert_eq!(f2.new_committed, "");
        assert_eq!(f2.new_streaming, " done\n");
        // A committed chunk, wherever it lands, always ends at a newline:
        // the gate is enforced by capping the boundary at the last '\n'.
        let f3 = stream.push("\nnext para");
        assert!(f3.new_committed.is_empty() || f3.new_committed.ends_with('\n'));
    }

    /// Deterministic LCG for reproducible chunk splits (no rand dep).
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
    }

    /// Split `source` into a random number of char-boundary chunks.
    fn random_chunks(source: &str, seed: u64) -> Vec<String> {
        let mut rng = Lcg(seed);
        let mut bounds: Vec<usize> = source.char_indices().map(|(i, _)| i).collect();
        if bounds.is_empty() {
            return vec![String::new()];
        }
        bounds.push(source.len());
        let mut chunks = Vec::new();
        let mut start = 0usize;
        for &b in bounds.iter() {
            if b == start {
                continue;
            }
            if rng.next().is_multiple_of(3) {
                chunks.push(source[start..b].to_string());
                start = b;
            }
        }
        if start < source.len() {
            chunks.push(source[start..].to_string());
        }
        chunks
    }

    /// Incremental-stream equivalence semantics: the incremental stream
    /// and the whole-source render must agree. Drives [`MarkdownStream`]
    /// with random char-boundary chunks and asserts:
    ///
    /// * at every step the render of the committed(+streaming) prefix is a
    ///   prefix of the full-source render — streaming never diverges and
    ///   never runs ahead of the final result;
    /// * `final_plain_text` before finalize equals the full-source render
    ///   (finalize-time backstop);
    /// * the delivered bytes reassemble the source exactly (incremental
    ///   delivery never re-emits and never drops).
    fn assert_streamed_equals_full(source: &str, seed: u64) {
        let chunks = random_chunks(source, seed);
        let full = render_plain_text(source);
        let mut stream = MarkdownStream::default();
        let mut delivered = String::new();
        for chunk in &chunks {
            let frame = stream.push(chunk);
            delivered.push_str(&frame.new_committed);
            delivered.push_str(&frame.new_streaming);
            let upto = stream.committed_upto().min(source.len());
            let committed_src = &source[..upto];
            assert!(
                full.starts_with(&render_plain_text(committed_src)),
                "committed prefix diverged from full render (seed {seed})"
            );
            let mut visible_src = String::from(committed_src);
            visible_src.push_str(stream.streaming_text());
            assert!(
                full.starts_with(&render_plain_text(&visible_src)),
                "streaming view ran ahead of the full render (seed {seed})"
            );
        }
        assert_eq!(
            stream.final_plain_text(),
            full,
            "finalize backstop must equal the whole-source render (seed {seed})"
        );
        delivered.push_str(&stream.finish().new_committed);
        assert_eq!(
            delivered, source,
            "incremental delivery must reassemble the source (seed {seed})"
        );
    }

    #[test]
    fn streamed_equals_full_paragraphs_and_lists() {
        for seed in 0..8u64 {
            assert_streamed_equals_full("hello world\n\nsecond para\n- a\n- b\n", seed);
        }
    }

    #[test]
    fn streamed_equals_full_table_holdback() {
        for seed in 0..8u64 {
            assert_streamed_equals_full(
                "| Name | Value |\n| --- | --- |\n| a | 1 |\n| b | 2 |\n",
                seed,
            );
        }
    }

    #[test]
    fn streamed_equals_full_fenced_code() {
        for seed in 0..8u64 {
            assert_streamed_equals_full("```rust\nfn main() {}\n```\n\nafter\n", seed);
        }
    }

    #[test]
    fn streamed_equals_full_reference_links() {
        for seed in 0..8u64 {
            assert_streamed_equals_full(
                "click [here][l] for more\n\n[l]: https://example.com\n",
                seed,
            );
        }
    }

    #[test]
    fn streamed_equals_full_mixed_document() {
        for seed in 0..8u64 {
            assert_streamed_equals_full(
                "# Title\n\nSome **bold** text with `code`.\n\n\
                 | A | B |\n| - | - |\n| 1 | 2 |\n\n\
                 ```sh\necho hi\n```\n\ndone.\n",
                seed,
            );
        }
    }

    #[test]
    fn throttled_coalesces_bursts_into_one_parse() {
        let mut stream = MarkdownStream::new_with_interval(64 * 1024, 100);
        assert!(stream.push_throttled("hello ").is_none());
        assert!(stream.push_throttled("world").is_none());
        assert!(stream.is_dirty());
        assert!(stream.prepare_frame(0, false).is_some());
        assert!(!stream.is_dirty());
        // Within the interval the next burst stays dirty without parsing.
        stream.push_throttled(" more");
        assert!(stream.prepare_frame(10, false).is_none());
        assert!(stream.is_dirty());
        // Forced completion drains without waiting for the interval.
        let forced = stream.prepare_frame(10, true).expect("forced parse drains");
        assert!(forced.new_committed.is_empty() || forced.new_committed.ends_with('\n'));
        assert!(!stream.is_dirty());
    }

    #[test]
    fn throttled_final_text_matches_immediate_path() {
        let source = "first para\n\nsecond para\n- a\n- b\n";
        let mut immediate = MarkdownStream::default();
        let mut immediate_text = String::new();
        for chunk in ["first ", "para\n\n", "second para\n", "- a\n- b\n"] {
            let frame = immediate.push(chunk);
            immediate_text.push_str(&frame.new_committed);
            immediate_text.push_str(&frame.new_streaming);
        }
        immediate_text.push_str(&immediate.finish().new_committed);

        let mut throttled = MarkdownStream::new_with_interval(64 * 1024, 1_000);
        let mut throttled_text = String::new();
        let mut now = 0u64;
        for chunk in ["first ", "para\n\n", "second para\n", "- a\n- b\n"] {
            assert!(throttled.push_throttled(chunk).is_none());
            now += 1;
            if let Some(frame) = throttled.prepare_frame(now, false) {
                throttled_text.push_str(&frame.new_committed);
                throttled_text.push_str(&frame.new_streaming);
            }
        }
        now += 10_000;
        if let Some(frame) = throttled.prepare_frame(now, false) {
            throttled_text.push_str(&frame.new_committed);
            throttled_text.push_str(&frame.new_streaming);
        }
        throttled_text.push_str(&throttled.finish().new_committed);
        assert_eq!(throttled_text, immediate_text);
        assert_eq!(throttled_text, source);
    }

    #[test]
    fn incremental_parse_stays_linear_on_long_history() {
        // Ten settled paragraphs followed by a growing tail: the incremental
        // path reparses only the tail, so total parser bytes stay linear.
        let mut history = String::new();
        for i in 0..10 {
            history.push_str(&format!("settled paragraph number {i} content\n\n"));
        }
        let mut stream = MarkdownStream::new_with_interval(64 * 1024, 0);
        stream.push_throttled(&history);
        stream.prepare_frame(0, true);
        let mut total_tail = 0usize;
        for i in 0..10 {
            let chunk = format!("tail line {i} addition\n");
            total_tail += chunk.len();
            stream.push_throttled(&chunk);
            stream.prepare_frame(u64::try_from(i).unwrap_or(0) + 1, true);
            let metrics = stream.parse_metrics();
            assert!(
                metrics.last_bytes <= total_tail + history.len() / 10,
                "incremental parse must not rescan the whole history"
            );
        }
        let metrics = stream.parse_metrics();
        assert!(metrics.incremental_parses > 0);
        assert!(
            metrics.total_bytes < (history.len() as u64) * 10,
            "total parse bytes must stay linear, got {}",
            metrics.total_bytes
        );
    }
}
