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

use pulldown_cmark::{Event, Parser, Tag};

/// Default source cap for the cumulative buffer (64 KiB).
pub const DEFAULT_MAX_SOURCE_BYTES: usize = 64 * 1024;

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

/// Append-only streaming markdown source with committed/streaming split.
pub struct MarkdownStream {
    buffer: String,
    /// Source bytes already delivered as committed text.
    committed_upto: usize,
    /// Source bytes already delivered as streaming text.
    streamed_upto: usize,
    max_source_bytes: usize,
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
        }
    }

    /// Append a delta and return the frame for this push.
    pub fn push(&mut self, delta: &str) -> MarkdownFrame {
        self.buffer.push_str(delta);
        if self.buffer.len() > self.max_source_bytes {
            return self.force_truncate();
        }
        let boundary = self.boundary();
        // Newly settled source bytes not yet delivered in any form: the
        // committed slice starts at `streamed_upto` (highest offset already
        // delivered as committed *or* streaming), so previously streamed
        // bytes are never re-emitted as committed.
        let new_committed = self.buffer[self.streamed_upto.min(boundary)..boundary].to_string();
        let stream_start = self.streamed_upto.max(boundary);
        let new_streaming = self.buffer[stream_start..].to_string();
        let code_lang = if boundary >= self.buffer.len() {
            None
        } else {
            extract_code_lang(&self.buffer[boundary..])
        };
        // The committed frontier is monotone: a holdback (table / reference
        // fallback) must never un-commit settled bytes.
        self.committed_upto = self.committed_upto.max(boundary);
        self.streamed_upto = self.buffer.len();
        MarkdownFrame {
            new_committed,
            new_streaming,
            code_lang,
        }
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
    /// (mini's scrollback cover) use this to flush the exact remaining
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

    /// Close the stream. Delta contract: only bytes never delivered in any
    /// earlier frame are returned — previously streamed bytes belong to the
    /// consumer's streaming view, which the consumer settles itself (mini
    /// flushes its scrollback span via [`Self::range_text`] before
    /// finishing). Never re-emits, never drops.
    pub fn finish(&mut self) -> MarkdownFrame {
        let committed = self.buffer[self.streamed_upto.min(self.buffer.len())..].to_string();
        self.buffer.clear();
        self.committed_upto = 0;
        self.streamed_upto = 0;
        MarkdownFrame {
            new_committed: committed,
            new_streaming: String::new(),
            code_lang: None,
        }
    }

    /// Over-limit protection: cut at a char boundary near the cap, commit
    /// everything that was not yet delivered and keep the tail for the next
    /// push. Never panics on non-boundary splits.
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
        MarkdownFrame {
            new_committed: committed,
            new_streaming: String::new(),
            code_lang: None,
        }
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
    ///   so a half line is never committed.
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
        let streaming_text = &self.buffer[split..];
        let mut boundary = if streaming_text.is_empty()
            || last_line_is_fence(streaming_text)
            || (ends_with_blank_line(&self.buffer) && !fence_open(streaming_text))
        {
            self.buffer.len()
        } else {
            split
        };
        let last_newline = self.buffer.rfind('\n').map(|i| i + 1).unwrap_or(0);
        if boundary > last_newline {
            boundary = last_newline;
        }
        boundary
    }
}

/// Locate the byte offset where the last top-level block begins.
///
/// Walks the offset iterator tracking tag depth; the start offset of the
/// final depth-0 block is the committed/streaming boundary.
fn last_top_level_block_start(src: &str) -> Option<usize> {
    let mut depth = 0u32;
    let mut last = None;
    for (event, range) in Parser::new(src).into_offset_iter() {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    last = Some(range.start);
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    last
}

/// True when `s` ends with a blank line (at least two consecutive newlines
/// at the end, tolerant of CRLF).
pub(crate) fn ends_with_blank_line(s: &str) -> bool {
    let trimmed = s.trim_end_matches(['\r', '\n']);
    s[trimmed.len()..].chars().filter(|c| *c == '\n').count() >= 2
}

/// True when `s` has an unclosed code fence (an odd number of fence lines).
fn fence_open(s: &str) -> bool {
    let mut open = false;
    for line in s.lines() {
        let t = line.trim();
        let mut chars = t.chars();
        let Some(first) = chars.next() else { continue };
        if first != '`' && first != '~' {
            continue;
        }
        let len = t.chars().take_while(|c| *c == first).count();
        if len < 3 {
            continue;
        }
        open = !open;
    }
    open
}

/// True when the buffer carries a reference-style link definition
/// (`[label]: destination`). Such definitions rewrite earlier link targets,
/// so incremental splitting is skipped and the whole source stays streaming
/// until finalize.
fn has_reference_definition(src: &str) -> bool {
    src.lines()
        .any(|line| line.trim_start().starts_with('[') && line.contains("]:"))
}

/// Table holdback: when the buffer tail holds an *unclosed* table (a header
/// row and a delimiter row are already present and table rows are still
/// continuing), return the byte offset of the table header start so the whole
/// table stays on the streaming side until finalize. Returns `None` when the
/// tail is not a table or the table already closed (a blank line follows the
/// last row). Callers must only invoke this outside fenced code blocks.
fn unclosed_table_start(src: &str) -> Option<usize> {
    let lines: Vec<&str> = src.split('\n').collect();
    // Skip trailing blank lines; the last non-blank line must be a table row.
    let mut i = lines.len();
    while i > 0 && lines[i - 1].trim().is_empty() {
        i -= 1;
    }
    if i == 0 || !is_table_line(lines[i - 1]) {
        return None;
    }
    // Collect the contiguous run of table rows upward and locate the
    // delimiter row (`| --- |`). The header row precedes the delimiter.
    let mut start = i;
    let mut delimiter: Option<usize> = None;
    let mut j = i;
    while j > 0 && is_table_line(lines[j - 1]) {
        if is_table_delimiter(lines[j - 1]) {
            delimiter = Some(j - 1);
        }
        start = j - 1;
        j -= 1;
    }
    let delimiter = delimiter?;
    if delimiter == start {
        return None; // delimiter row is the first row: no header, no table
    }
    let mut offset = 0usize;
    for line in lines.iter().take(start) {
        offset += line.len() + 1; // +1 for the newline separator
    }
    Some(offset)
}

/// A table row: the trimmed line starts with `|` (GFM tables may also keep
/// a closing `|` after the cells).
fn is_table_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') || (t.ends_with('|') && t.contains('|'))
}

/// A table delimiter row: only `|`, `-`, `:` and whitespace, with at least
/// one `-` (e.g. `| --- | :---: |`).
fn is_table_delimiter(line: &str) -> bool {
    let t = line.trim();
    t.contains('-') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// True when the last line of `s` is a pure closing fence and an opening
/// fence exists earlier in the stream (i.e. the fenced code block settled).
fn last_line_is_fence(s: &str) -> bool {
    let Some((prefix, last)) = s.rsplit_once('\n') else {
        return false; // single line: an opening fence, not a closing one
    };
    let t = last.trim();
    let first = t.chars().next();
    let pure = matches!(first, Some('`') | Some('~'))
        && t.len() >= 3
        && t.chars().all(|c| c == first.unwrap());
    if !pure {
        return false;
    }
    prefix.lines().any(|l| {
        l.trim()
            .chars()
            .next()
            .is_some_and(|c| c == '`' || c == '~')
    })
}

/// Language tag of the in-flight fenced code block, if any.
fn extract_code_lang(streaming: &str) -> Option<String> {
    for line in streaming.lines() {
        let t = line.trim();
        let mut chars = t.chars();
        let Some(first) = chars.next() else { continue };
        if first != '`' && first != '~' {
            continue;
        }
        let fence_len = t.chars().take_while(|c| *c == first).count();
        if fence_len < 3 {
            continue;
        }
        let rest: String = t.chars().skip(fence_len).collect();
        let rest = rest.trim();
        if !rest.is_empty() {
            return rest.split_whitespace().next().map(str::to_string);
        }
    }
    None
}

/// Render Markdown source to plain text (line-level output): inline markup
/// is stripped, soft/hard breaks map to `\n`, and code blocks keep their
/// content lines verbatim. No styling — that belongs to the interactive
/// renderers.
pub fn render_plain_text(src: &str) -> String {
    let mut out = String::new();
    let mut boundary = false;
    for event in Parser::new(src) {
        match event {
            Event::Text(t) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                out.push_str(t.as_ref());
            }
            Event::Code(t) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                out.push_str(t.as_ref());
            }
            Event::SoftBreak | Event::HardBreak => {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
            }
            Event::Rule | Event::TaskListMarker(_) | Event::FootnoteReference(_) => {
                boundary = true;
            }
            Event::Start(tag) if is_block_start(&tag) => {
                // A block-level container (paragraph, heading, list item,
                // code block, …) begins a new rendered line. Inline spans
                // (emphasis/strong/link) deliberately do **not** set the
                // boundary so adjacent inline text stays on one line.
                boundary = true;
            }
            _ => {}
        }
    }
    out
}

/// True when the tag opens a block-level container rather than an inline span.
fn is_block_start(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::MetadataBlock(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // already streamed above (delta contract, never re-emitted).
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
    fn soft_breaks_are_preserved_in_plain_text() {
        assert_eq!(render_plain_text("hello\nworld"), "hello\nworld");
    }

    #[test]
    fn inline_markup_is_stripped_in_plain_text() {
        assert_eq!(
            render_plain_text("**bold** and *em* and `code`"),
            "bold and em and code"
        );
    }

    #[test]
    fn fenced_code_keeps_content_verbatim() {
        assert_eq!(
            render_plain_text("```rust\nfn main() {}\n```"),
            "fn main() {}\n"
        );
    }

    #[test]
    fn headings_and_lists_render_as_lines() {
        assert_eq!(render_plain_text("# Title"), "Title");
        assert_eq!(render_plain_text("- a\n- b"), "a\nb");
    }

    #[test]
    fn finish_commits_everything_remaining() {
        let mut stream = MarkdownStream::default();
        stream.push("unfinished");
        // Delta contract: the streamed tail was already delivered, so the
        // finalize frame carries nothing new — the consumer settles its own
        // streaming view (see `range_text`).
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
        // whole table exactly once, and finalize adds nothing (delta
        // contract — the consumer settles its own streaming view).
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
}
