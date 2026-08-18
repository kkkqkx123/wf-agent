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

/// A frame of streaming markdown output for one `push`/`finish` call.
///
/// Both text fields are **source text** (never width-fixed render caches);
/// reflow and styling belong to Stage 4's `HistoryLine` and Stage 6/7.
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
        let new_committed =
            self.buffer[self.streamed_upto.min(boundary)..boundary].to_string();
        let stream_start = self.streamed_upto.max(boundary);
        let new_streaming = self.buffer[stream_start..].to_string();
        let code_lang = if boundary >= self.buffer.len() {
            None
        } else {
            extract_code_lang(&self.buffer[boundary..])
        };
        self.committed_upto = boundary;
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

    /// Close the stream: everything remaining is committed.
    pub fn finish(&mut self) -> MarkdownFrame {
        let committed =
            self.buffer[self.committed_upto.min(self.buffer.len())..].to_string();
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
    fn boundary(&self) -> usize {
        if self.buffer.is_empty() {
            return 0;
        }
        let split = last_top_level_block_start(&self.buffer).unwrap_or(self.buffer.len());
        let streaming_text = &self.buffer[split..];
        if streaming_text.is_empty()
            || last_line_is_fence(streaming_text)
            || (ends_with_blank_line(&self.buffer) && !fence_open(streaming_text))
        {
            self.buffer.len()
        } else {
            split
        }
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
            .map_or(false, |c| c == '`' || c == '~')
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
/// content lines verbatim. No styling — that belongs to Stage 6/7.
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

        // The closing fence settles the block.
        let frame = stream.push("}\n```");
        assert_eq!(frame.new_committed, "}\n```");
        assert_eq!(frame.new_streaming, "");
        assert_eq!(frame.code_lang, None);
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
        let frame = stream.finish();
        assert_eq!(frame.new_committed, "unfinished");
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
}
