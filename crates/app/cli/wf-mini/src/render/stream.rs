//! Streaming markdown renderer with settle-boundary emission and degrade
//! to verbatim passthrough.

use tui_markdown::markdown::{has_table_risk, settled_boundary};
use wf_cli_shared::display::MarkdownMode;

use super::strip::strip_terminal_escapes;
use super::{render_plain_text, render_styled};

fn common_prefix_len(a: &str, b: &str) -> usize {
    let bytes = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    let mut len = bytes;
    while len > 0 && !b.is_char_boundary(len) {
        len -= 1;
    }
    len
}

/// Newly rendered bytes since `emitted`, for an append-only stdout.
///
/// The fast path — `emitted` is a prefix of `target` — is the streaming
/// contract: settled bytes never change, so a delta appends. The
/// common-prefix fallback exists because that contract can break: a
/// construct that opened on an already-settled line and closed on a later
/// one (emphasis spanning a soft break) rewrites the line, and a setext
/// underline turns the preceding paragraph into a heading. Bytes already
/// written to a terminal cannot be retracted, so the fallback re-sends
/// from the common prefix: the stale prefix stays on screen and the suffix
/// repeats itself once, then the cursor advances and later deltas diff
/// cleanly again. It is a last-resort repair, not a normal path — holding
/// the source back before such a rewrite reaches stdout is the only real
/// fix.
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
/// are never re-emitted. `Plain`/`Styled` emit the settled region only —
/// the cut follows [`settled_boundary`], so an in-flight block (the
/// current paragraph, an open fence, an unclosed table, a pending
/// reference definition) stays held back until no later delta can rewrite
/// it. `Raw` never re-renders its input, so it cuts at the last newline
/// instead. Whatever was emitted is final: bytes on stdout cannot be
/// retracted, so `diff_suffix`'s common-prefix path stays as the
/// last-resort repair for a rewrite the hold-backs did not foresee.
///
/// When the accumulated source passes `DEFAULT_MAX_SOURCE_BYTES`, the
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

    fn render_source(&self, src: &str) -> String {
        match self.mode {
            MarkdownMode::Raw => src.to_owned(),
            MarkdownMode::Plain => render_plain_text(src),
            MarkdownMode::Styled if self.color => render_styled(src),
            MarkdownMode::Styled => render_plain_text(src),
        }
    }

    fn render_full(&self) -> String {
        self.render_source(&self.source)
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
        // A tail line with `|` can still turn into a table: the settled
        // boundary only recognizes a table once its delimiter row arrived,
        // so this conservative tail check holds the source back until then.
        if has_table_risk(&self.source) {
            return String::new();
        }
        self.emit_settled()
    }

    /// Render the settled region and return its new bytes.
    ///
    /// The cut is the last point a later delta cannot rewrite, so the
    /// emitted text is stable and `diff_suffix`'s prefix fast path holds.
    /// `Raw` streams the source verbatim and never re-renders it, so the
    /// last newline is a safe cut there.
    fn emit_settled(&mut self) -> String {
        let cut = match self.mode {
            MarkdownMode::Raw => self.source.rfind('\n').map_or(0, |pos| pos + 1),
            _ => settled_boundary(&self.source),
        };
        if cut == 0 {
            return String::new();
        }
        let target = self.render_source(&self.source[..cut]);
        let new = diff_suffix(&self.emitted, &target);
        self.emitted = target;
        new
    }

    /// Switch to passthrough at the source cap: emit everything up to the
    /// last newline once, then carry the rendered but not-yet-emitted tail
    /// into the line buffer so no content is lost across the switch.
    fn degrade(&mut self) -> String {
        let mut full = self.render_full();
        restore_trailing_newline(&self.source, &mut full);
        let seam = full.rfind('\n').map_or(0, |pos| pos + 1);
        let out = diff_suffix(&self.emitted, &full[..seam]);
        self.line_buffer.push_str(&full[seam..]);
        self.emitted = full[..seam].to_string();
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
    use tui_markdown::markdown::has_reference_definition;
    use wf_cli_shared::display::MarkdownMode as Mode;

    #[allow(unused_imports)]
    use tui_markdown::markdown::plain::render_plain_text;

    #[test]
    fn oversized_source_degrades_without_loss_or_reemit() {
        let mut renderer = StreamRenderer::new(Mode::Plain, false);
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
        let mut renderer = StreamRenderer::new(Mode::Plain, false);
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
    fn stream_styled_quote_holds_the_marker_until_the_block_settles() {
        let mut stream = StreamRenderer::new(Mode::Styled, true);
        assert_eq!(stream.push("> qu"), "");
        assert_eq!(stream.push("oted\n"), "");
        // The quoted paragraph is one in-flight block: it emits whole,
        // marker included, once a later delta can no longer rewrite it.
        let out = stream.flush();
        assert_eq!(strip_terminal_escapes(&out), "> quoted\n");
        assert!(!out.contains("> \n"), "{out:?}");
    }

    #[test]
    fn diff_suffix_falls_back_to_the_common_prefix_on_a_rewrite() {
        // Reachable through streaming: an emphasis opened on a settled line
        // and closed on a later one rewrites the emitted line, so the
        // emitted text is no longer a prefix of the new render.
        assert_eq!(
            diff_suffix("hello *wor\n", "hello wor\nld end\n"),
            "wor\nld end\n"
        );
        // The fast path stays the normal case.
        assert_eq!(diff_suffix("hello ", "hello world\n"), "world\n");
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
    fn stream_plain_holds_the_in_flight_paragraph() {
        let mut stream = StreamRenderer::new(Mode::Plain, false);
        assert_eq!(stream.push("**hello** "), "");
        assert_eq!(stream.push("world\n"), "");
        assert_eq!(stream.push("tail"), "");
        // The blank line settles the whole paragraph (the soft-broken
        // lines are one block); the next open tail stays back.
        assert_eq!(stream.push("\n\n"), "hello world\ntail");
        assert_eq!(stream.push("more"), "");
        assert_eq!(stream.flush(), "\nmore");
    }

    #[test]
    fn stream_never_reemits_bytes() {
        let mut stream = StreamRenderer::new(Mode::Plain, false);
        let mut seen = String::new();
        seen.push_str(&stream.push("hello "));
        seen.push_str(&stream.push("world\n"));
        seen.push_str(&stream.push("again\n"));
        seen.push_str(&stream.flush());
        assert_eq!(seen, "hello world\nagain\n");
    }

    #[test]
    fn stream_styled_without_color_degrades_to_plain() {
        let mut stream = StreamRenderer::new(Mode::Styled, false);
        assert_eq!(stream.push("**hi**\n"), "");
        assert_eq!(stream.flush(), "hi\n");
    }

    #[test]
    fn stream_styled_with_color_emits_sgr() {
        let mut stream = StreamRenderer::new(Mode::Styled, true);
        assert_eq!(stream.push("**hi**\n"), "");
        let out = stream.flush();
        assert!(out.contains("\x1b[1m"), "{out:?}");
        assert!(out.contains("\x1b[0m"), "{out:?}");
    }

    #[test]
    fn stream_sanitizes_model_escapes() {
        let mut stream = StreamRenderer::new(Mode::Plain, false);
        assert_eq!(stream.push("\x1b[2J**hi**\n"), "");
        assert_eq!(stream.flush(), "hi\n");
    }

    #[test]
    fn stream_holds_emphasis_that_spans_a_line_break() {
        // The emphasis opens on one line and closes on the next: under the
        // old last-newline cut the first line went out with its literal
        // `*` and the fallback re-sent the rest. The paragraph now stays
        // in flight until a later delta cannot rewrite it.
        let mut stream = StreamRenderer::new(Mode::Plain, false);
        assert_eq!(stream.push("hello *wor\n"), "");
        assert_eq!(stream.push("ld* end\n"), "");
        assert_eq!(stream.flush(), "hello wor\nld end\n");
    }

    #[test]
    fn stream_holds_a_setext_underline_until_the_heading_settles() {
        let mut stream = StreamRenderer::new(Mode::Styled, true);
        assert_eq!(stream.push("Title\n"), "");
        assert_eq!(stream.push("=====\n"), "");
        // One render, emitted once: the paragraph was never sent out as
        // plain text before the underline turned it into a heading.
        assert_eq!(strip_terminal_escapes(&stream.flush()), "# Title\n");
    }

    #[test]
    fn stream_holds_back_unclosed_tables_until_flush() {
        let mut stream = StreamRenderer::new(Mode::Plain, false);
        assert_eq!(stream.push("| a | b |\n"), "");
        assert_eq!(stream.push("| --- | --- |\n"), "");
        assert_eq!(stream.push("| 1 | 2 |\n"), "");
        assert_eq!(stream.flush(), "a\nb\n1\n2\n");
    }

    #[test]
    fn stream_releases_hold_when_tail_has_no_pipe() {
        let mut stream = StreamRenderer::new(Mode::Plain, false);
        assert_eq!(stream.push("| a | b |\n"), "");
        assert_eq!(stream.push("| --- | --- |\n"), "");
        assert_eq!(stream.push("| 1 | 2 |\n"), "");
        // The blank line closes the table, so the table becomes settled
        // when the following paragraph opens.
        assert_eq!(stream.push("\nplain text\n"), "a\nb\n1\n2");
        assert_eq!(stream.flush(), "\nplain text\n");
    }
}
