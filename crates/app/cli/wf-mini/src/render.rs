//! Append-only markdown rendering for the native session.
//!
//! The full TUI re-renders settled blocks into a viewport it owns, so it can
//! revise optimistic styling on the next frame. The native session cannot:
//! bytes written to stdout stay on the scrollback. Rendering here is
//! therefore conservative:
//!
//! * every mode strips incoming terminal escapes first, so model output can
//!   never drive the terminal (cursor moves, clear, title, hyperlinks);
//! * `Raw` streams sanitized text verbatim (historical behavior);
//! * `Plain` strips markdown markup without adding escapes and emits
//!   complete lines only;
//! * `Styled` wraps markdown spans in ANSI SGR and degrades to plain text
//!   when color is unavailable. Like plain it emits complete lines only, so
//!   a half-typed construct is never styled optimistically.
//!
//! The transcript always keeps the sanitized raw text, never the rendered
//! form, so display choices never leak into model context.

use crossterm::style::{Attribute, Color, ContentStyle, StyledContent, Stylize};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

use tui_markdown::markdown::{has_table_risk, parser_options};
use wf_cli_shared::display::MarkdownMode;

pub use tui_markdown::markdown::has_reference_definition;

/// Strip terminal escapes from model text so it can never drive the
/// terminal. CSI (`ESC [ ... final`) and OSC (`ESC ] ... BEL/ST`) sequences
/// are removed whole; a trailing partial sequence is dropped with the delta
/// that carries it. Other C0 controls (including bare `ESC`, `CR`, `DEL`)
/// are dropped; `\n` and `\t` survive and multibyte UTF-8 passes through
/// untouched.
pub fn strip_terminal_escapes(text: &str) -> String {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        if bytes[i] == 0x1b {
            if i + 1 < len && bytes[i + 1] == b'[' {
                let mut j = i + 2;
                while j < len && is_csi_body(bytes[j]) {
                    j += 1;
                }
                if j < len && bytes[j] >= 0x40 && bytes[j] <= 0x7e {
                    j += 1;
                }
                i = j;
                continue;
            }
            if i + 1 < len && bytes[i + 1] == b']' {
                let mut j = i + 2;
                while j < len {
                    if bytes[j] == 0x07 {
                        j += 1;
                        break;
                    }
                    if bytes[j] == 0x1b && j + 1 < len && bytes[j + 1] == b'\\' {
                        j += 2;
                        break;
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            // Bare ESC (or an unknown sequence leader): drop the byte
            // itself. Any payload without its introducer is inert text.
            i += 1;
            continue;
        }
        let b = bytes[i];
        if b == b'\n' || b == b'\t' || b >= 0x20 && b != 0x7f {
            out.push(b);
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned())
}

fn is_csi_body(b: u8) -> bool {
    (0x30..=0x3f).contains(&b) || (0x20..=0x2f).contains(&b)
}

/// Plain-text markdown rendering shared with the TUI (`render_plain_text`).
pub use tui_markdown::markdown::plain::render_plain_text;

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

/// Inline span kinds that contribute ANSI styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inline {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
}

/// SGR parameter sequence for the open inline spans, e.g. `1;36`.
fn inline_style(stack: &[Inline], heading: bool) -> ContentStyle {
    let mut style = ContentStyle::new();
    if stack.contains(&Inline::Bold) || heading {
        style.attributes.set(Attribute::Bold);
    }
    if stack.contains(&Inline::Italic) {
        style.attributes.set(Attribute::Italic);
    }
    if stack.contains(&Inline::Strike) {
        style.attributes.set(Attribute::CrossedOut);
    }
    if stack.contains(&Inline::Link) {
        style.attributes.set(Attribute::Underlined);
    }
    if stack.contains(&Inline::Code) {
        style.foreground_color = Some(Color::Cyan);
    }
    style
}

/// Dim helper for fences, quotes and rules.
fn dim_span(text: &str) -> String {
    StyledContent::new(
        {
            let mut style = ContentStyle::new();
            style.attributes.set(Attribute::Dim);
            style
        },
        text,
    )
    .to_string()
}

/// Cyan helper for code spans and fenced content.
fn cyan_span(text: &str) -> String {
    text.with(Color::Cyan).to_string()
}

/// Bold helper for headings.
fn bold_span(text: &str) -> String {
    text.attribute(Attribute::Bold).to_string()
}

fn heading_depth(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn emit_styled_text(
    out: &mut String,
    boundary: &mut bool,
    text: &str,
    inline: &[Inline],
    heading: bool,
) {
    if *boundary && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    *boundary = false;
    let style = inline_style(inline, heading);
    if style.foreground_color.is_none() && style.attributes.is_empty() {
        out.push_str(text);
    } else {
        out.push_str(&style.apply(text).to_string());
    }
}

/// Render markdown source with ANSI SGR styling for a line-discipline
/// terminal. Every styled span self-resets, so styling never leaks across
/// lines or into later output. Link targets are dropped (only the link text
/// is shown, underlined); fenced code keeps its content verbatim in cyan.
pub fn render_styled(src: &str) -> String {
    let mut out = String::new();
    let mut boundary = false;
    let mut inline: Vec<Inline> = Vec::new();
    let mut heading = false;
    let mut in_code_block = false;
    let mut in_table_head = false;
    let mut in_blockquote = false;

    for event in Parser::new_ext(src, parser_options()) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Emphasis => inline.push(Inline::Italic),
                Tag::Strong => inline.push(Inline::Bold),
                Tag::Strikethrough => inline.push(Inline::Strike),
                Tag::Link { .. } => inline.push(Inline::Link),
                Tag::Image { .. } => inline.push(Inline::Link),
                Tag::Heading { level, .. } => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    boundary = false;
                    out.push_str(&bold_span(&format!(
                        "{} ",
                        "#".repeat(heading_depth(level))
                    )));
                    heading = true;
                }
                Tag::CodeBlock(kind) => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    boundary = false;
                    in_code_block = true;
                    let lang = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    out.push_str(&dim_span(&format!("```{lang}")));
                    out.push('\n');
                }
                Tag::BlockQuote(_) => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    boundary = false;
                    in_blockquote = true;
                    out.push_str(&dim_span("> "));
                }
                Tag::TableHead => {
                    boundary = true;
                    in_table_head = true;
                }
                _ if is_block_start(&tag) => {
                    boundary = true;
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Emphasis => {
                    inline.retain(|kind| *kind != Inline::Italic);
                }
                TagEnd::Strong => {
                    inline.retain(|kind| *kind != Inline::Bold);
                }
                TagEnd::Strikethrough => {
                    inline.retain(|kind| *kind != Inline::Strike);
                }
                TagEnd::Link | TagEnd::Image => {
                    inline.retain(|kind| *kind != Inline::Link);
                }
                TagEnd::Heading(_) => {
                    heading = false;
                    boundary = true;
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(&dim_span("```"));
                    boundary = true;
                }
                TagEnd::BlockQuote(_) => {
                    in_blockquote = false;
                    boundary = true;
                }
                TagEnd::TableHead => {
                    in_table_head = false;
                    boundary = true;
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    if boundary && !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    boundary = false;
                    out.push_str(&cyan_span(text.as_ref()));
                } else {
                    let head = heading || in_table_head;
                    emit_styled_text(&mut out, &mut boundary, text.as_ref(), &inline, head);
                }
            }
            Event::Code(code) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                out.push_str(&cyan_span(code.as_ref()));
            }
            Event::SoftBreak | Event::HardBreak => {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                if in_blockquote {
                    out.push_str(&dim_span("> "));
                }
            }
            Event::Rule => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&dim_span("---"));
                boundary = true;
            }
            Event::TaskListMarker(checked) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                out.push_str(if checked { "[x] " } else { "[ ] " });
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
            }
            _ => {}
        }
    }
    out
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    let bytes = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    let mut len = bytes;
    while len > 0 && !b.is_char_boundary(len) {
        len -= 1;
    }
    len
}

fn diff_suffix(emitted: &str, target: &str) -> String {
    if let Some(stripped) = target.strip_prefix(emitted) {
        return stripped.to_string();
    }
    let common = common_prefix_len(emitted, target);
    target[common..].to_string()
}

/// The parser folds a paragraph's trailing terminator newline into the
/// paragraph end (no break event), so a source ending in a single newline
/// renders without it. A single trailing newline is a real settled line
/// break: restore it so the line emits whole. A blank-line terminator is a
/// block settlement and gets no trailing newline.
fn restore_trailing_newline(source: &str, rendered: &mut String) {
    if source.ends_with('\n')
        && !rendered.is_empty()
        && !rendered.ends_with('\n')
        && !source.ends_with("\n\n")
    {
        rendered.push('\n');
    }
}

/// Source cap before the streaming renderer degrades to verbatim
/// passthrough, mirroring the TUI streaming cap.
const DEFAULT_MAX_SOURCE_BYTES: usize = 64 * 1024;

/// Streaming markdown renderer for `Plain`/`Styled` modes. Keeps the full
/// sanitized source, re-renders on every delta (turn outputs are small; the
/// parse is microseconds) and diffs against already-emitted output so bytes
/// are never re-emitted. Complete lines only: the in-flight tail stays held
/// back until a newline settles it or `flush` drains it at a boundary.
/// Two constructs hold the whole source back because a later delta can
/// retroactively rewrite already-rendered bytes: reference-style link
/// definitions and unclosed tables (any tail line containing `|`).
///
/// When the accumulated source passes [`DEFAULT_MAX_SOURCE_BYTES`], the
/// renderer emits the settled output once and degrades to verbatim
/// passthrough: later deltas skip markdown rendering and stream through a
/// single-line buffer with the same complete-lines-only discipline, so
/// memory stays bounded (cap plus one line) on oversized outputs. The seam
/// line at the switch keeps its already-rendered prefix; only inline SGR on
/// that one line is lost in styled mode.
pub struct StreamRenderer {
    mode: MarkdownMode,
    color: bool,
    source: String,
    emitted: String,
    degraded: bool,
    /// Unsettled line in passthrough mode (rendered seam tail + raw deltas).
    line_buffer: String,
}

impl StreamRenderer {
    pub fn new(mode: MarkdownMode, color: bool) -> Self {
        Self {
            mode,
            color,
            source: String::new(),
            emitted: String::new(),
            degraded: false,
            line_buffer: String::new(),
        }
    }

    fn render_full(&self) -> String {
        match self.mode {
            MarkdownMode::Raw => self.source.clone(),
            MarkdownMode::Plain => render_plain_text(&self.source),
            MarkdownMode::Styled if self.color => render_styled(&self.source),
            MarkdownMode::Styled => render_plain_text(&self.source),
        }
    }

    /// Feed one delta, returning the newly settled output for stdout.
    pub fn push(&mut self, delta: &str) -> String {
        let clean = strip_terminal_escapes(delta);
        if clean.is_empty() {
            return String::new();
        }
        if self.degraded {
            return self.push_passthrough(&clean);
        }
        self.source.push_str(&clean);
        if self.source.len() > DEFAULT_MAX_SOURCE_BYTES {
            return self.degrade();
        }
        if has_reference_definition(&self.source) || has_table_risk(&self.source) {
            return String::new();
        }
        self.emit_settled()
    }

    /// Emit rendered output up to the last newline, advancing the emitted
    /// cursor so bytes are never re-sent.
    fn emit_settled(&mut self) -> String {
        let mut full = self.render_full();
        restore_trailing_newline(&self.source, &mut full);
        let target = if self.source.ends_with('\n') {
            full.clone()
        } else {
            match full.rfind('\n') {
                Some(pos) => full[..=pos].to_string(),
                None => String::new(),
            }
        };
        let new = diff_suffix(&self.emitted, &target);
        self.emitted = target;
        new
    }

    /// Switch to passthrough at the source cap: emit the settled output
    /// once, then carry the rendered but not-yet-emitted tail into the line
    /// buffer so no content is lost across the switch.
    fn degrade(&mut self) -> String {
        let out = self.emit_settled();
        if !self.source.ends_with('\n') {
            let mut full = self.render_full();
            restore_trailing_newline(&self.source, &mut full);
            let remainder = match full.rfind('\n') {
                Some(pos) => &full[pos + 1..],
                None => full.as_str(),
            };
            self.line_buffer.push_str(remainder);
        }
        self.degraded = true;
        out
    }

    /// Passthrough after the cap: raw text through a single-line buffer,
    /// complete lines only.
    fn push_passthrough(&mut self, clean: &str) -> String {
        self.line_buffer.push_str(clean);
        let Some(pos) = self.line_buffer.rfind('\n') else {
            return String::new();
        };
        let out = self.line_buffer[..=pos].to_string();
        self.line_buffer.drain(..=pos);
        out
    }

    /// Drain everything, including the in-flight tail, at a boundary
    /// (iteration end, tool event, turn end).
    pub fn flush(&mut self) -> String {
        if self.degraded {
            return std::mem::take(&mut self.line_buffer);
        }
        if self.source.is_empty() {
            return String::new();
        }
        if has_reference_definition(&self.source) {
            let full = self.render_full();
            let new = diff_suffix(&self.emitted, &full);
            self.emitted = full;
            return new;
        }
        let mut full = self.render_full();
        restore_trailing_newline(&self.source, &mut full);
        let new = diff_suffix(&self.emitted, &full);
        self.emitted = full;
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_csi_color_and_cursor_sequences() {
        assert_eq!(strip_terminal_escapes("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_terminal_escapes("a\x1b[2J\x1b[Hb"), "ab");
    }

    #[test]
    fn strips_osc_hyperlink_keeping_text() {
        assert_eq!(
            strip_terminal_escapes("\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\"),
            "link"
        );
        assert_eq!(strip_terminal_escapes("a\x1b]0;title\x07b"), "ab");
    }

    #[test]
    fn drops_bare_esc_and_controls_but_keeps_text() {
        assert_eq!(strip_terminal_escapes("a\x1bb"), "ab");
        assert_eq!(strip_terminal_escapes("a\rb\nc\td"), "ab\nc\td");
        assert_eq!(strip_terminal_escapes("你好\x1b[31m世界"), "你好世界");
    }

    #[test]
    fn drops_trailing_partial_sequence() {
        assert_eq!(strip_terminal_escapes("abc\x1b["), "abc");
        assert_eq!(strip_terminal_escapes("abc\x1b[31"), "abc");
    }

    #[test]
    fn oversized_source_degrades_without_loss_or_reemit() {
        let mut renderer = StreamRenderer::new(MarkdownMode::Plain, false);
        let big = "a".repeat(70_000);
        let mut out = String::new();
        out.push_str(&renderer.push(&big[..60_000]));
        out.push_str(&renderer.push(&big[60_000..]));
        out.push_str(&renderer.push(" END\n"));
        out.push_str(&renderer.push("more\n"));
        out.push_str(&renderer.flush());
        assert_eq!(out.matches('a').count(), 70_000, "no loss, no re-emit");
        assert!(out.contains(" END\n"));
        assert!(out.contains("more\n"));
        // Deltas after the cap never consult the markdown renderer again:
        // markup passes through verbatim.
        assert_eq!(renderer.push("**raw**\n"), "**raw**\n");
    }

    #[test]
    fn oversized_source_seam_keeps_incomplete_tail_line() {
        let mut renderer = StreamRenderer::new(MarkdownMode::Plain, false);
        let mut out = String::new();
        out.push_str(&renderer.push(&"a".repeat(60_000)));
        out.push_str(&renderer.push(&"b".repeat(10_000)));
        // Still one incomplete line crossing the cap; nothing emitted yet.
        assert_eq!(out, "");
        out.push_str(&renderer.push(" end\n"));
        out.push_str(&renderer.flush());
        assert_eq!(out.matches('a').count(), 60_000);
        assert_eq!(out.matches('b').count(), 10_000);
        assert!(out.contains("b end\n"));
    }

    #[test]
    fn plain_strips_markup_and_keeps_code() {
        assert_eq!(
            render_plain_text("**bold** and *em* and `code`"),
            "bold and em and code"
        );
        assert_eq!(render_plain_text("```rust\nfn main() {}\n```"), "fn main() {}\n");
        assert_eq!(render_plain_text("# Title"), "Title");
    }

    #[test]
    fn styled_wraps_spans_and_resets() {
        let styled = render_styled("**bold** and `code`");
        assert!(styled.contains("\x1b[1m"), "{styled:?}");
        assert!(styled.contains("38;5;14"), "{styled:?}");
        assert!(styled.contains("\x1b[0m"), "{styled:?}");
        assert!(!styled.contains("**"), "{styled:?}");
        assert_eq!(strip_terminal_escapes(&styled), "bold and code");
    }

    #[test]
    fn styled_resets_before_newlines() {
        let styled = render_styled("**bold**\nnext");
        assert!(styled.contains("bold\x1b[0m"), "{styled:?}");
    }

    #[test]
    fn styled_semantics_match_plain_after_strip() {
        for src in [
            "**bold** and `code`",
            "body *em* text",
            "**bold** and *em* and `code`",
        ] {
            assert_eq!(
                strip_terminal_escapes(&render_styled(src)),
                render_plain_text(src),
                "{src:?}"
            );
        }
    }

    #[test]
    fn reference_definitions_hold_back() {
        assert!(has_reference_definition(
            "click [here][l]\n\n[l]: https://x.test\n"
        ));
        assert!(has_reference_definition("see [a][b] now"));
        assert!(!has_reference_definition("plain **bold** text\n"));
    }

    #[test]
    fn stream_plain_emits_complete_lines_only() {
        let mut stream = StreamRenderer::new(MarkdownMode::Plain, false);
        assert_eq!(stream.push("**hello** "), "");
        assert_eq!(stream.push("world\n"), "hello world\n");
        assert_eq!(stream.push("tail"), "");
        assert_eq!(stream.flush(), "tail");
    }

    #[test]
    fn stream_never_reemits_bytes() {
        let mut stream = StreamRenderer::new(MarkdownMode::Plain, false);
        let mut seen = String::new();
        seen.push_str(&stream.push("hello "));
        seen.push_str(&stream.push("world\n"));
        seen.push_str(&stream.push("again\n"));
        seen.push_str(&stream.flush());
        assert_eq!(seen, "hello world\nagain\n");
    }

    #[test]
    fn stream_styled_without_color_degrades_to_plain() {
        let mut stream = StreamRenderer::new(MarkdownMode::Styled, false);
        assert_eq!(stream.push("**hi**\n"), "hi\n");
    }

    #[test]
    fn stream_styled_with_color_emits_sgr() {
        let mut stream = StreamRenderer::new(MarkdownMode::Styled, true);
        let out = stream.push("**hi**\n");
        assert!(out.contains("\x1b[1m"), "{out:?}");
        assert!(out.contains("\x1b[0m"), "{out:?}");
    }

    #[test]
    fn stream_sanitizes_model_escapes() {
        let mut stream = StreamRenderer::new(MarkdownMode::Plain, false);
        assert_eq!(stream.push("\x1b[2J**hi**\n"), "hi\n");
    }

    #[test]
    fn plain_renders_table_cells_and_task_markers() {
        assert_eq!(
            render_plain_text("| a | b |\n| --- | --- |\n| 1 | 2 |\n"),
            "a\nb\n1\n2"
        );
        assert_eq!(
            render_plain_text("- [x] done\n- [ ] todo\n"),
            "[x] done\n[ ] todo"
        );
        assert_eq!(render_plain_text("~~gone~~"), "gone");
    }

    #[test]
    fn styled_renders_strikethrough_and_task_markers() {
        let styled = render_styled("~~gone~~\n\n- [x] done\n- [ ] left\n");
        assert!(styled.contains("\x1b[9m"), "{styled:?}");
        assert!(styled.contains("[x] done"), "{styled:?}");
        assert!(styled.contains("[ ] left"), "{styled:?}");
        assert_eq!(strip_terminal_escapes(&styled), "gone\n[x] done\n[ ] left");
    }

    #[test]
    fn stream_holds_back_unclosed_tables_until_flush() {
        let mut stream = StreamRenderer::new(MarkdownMode::Plain, false);
        assert_eq!(stream.push("| a | b |\n"), "");
        assert_eq!(stream.push("| --- | --- |\n"), "");
        assert_eq!(stream.push("| 1 | 2 |\n"), "");
        assert_eq!(stream.flush(), "a\nb\n1\n2\n");
    }

    #[test]
    fn stream_releases_hold_when_tail_has_no_pipe() {
        let mut stream = StreamRenderer::new(MarkdownMode::Plain, false);
        assert_eq!(stream.push("| a | b |\n"), "");
        assert_eq!(stream.push("| --- | --- |\n"), "");
        assert_eq!(stream.push("| 1 | 2 |\n"), "");
        assert_eq!(stream.push("\nplain text\n"), "a\nb\n1\n2\nplain text\n");
    }
}
