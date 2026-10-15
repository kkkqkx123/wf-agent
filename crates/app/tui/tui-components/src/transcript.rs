//! Transcript primitives shared by the TUI footer prompt view (inline form)
//! and the full-screen TUI. Unrelated to the `wf-mini` binary.
//!
//! The core type is [`HistoryLine`]: it holds the **source text** (never a
//! width-fixed render cache) so that `display_lines(width)` recomputes the
//! line wrapping for the current terminal width on every call — the
//! resize-reflow ground rule. A line carries a [`LineRender`] mode: `Plain`
//! reflows the source verbatim, `Markdown` interprets it as Markdown through
//! the shared styled renderer so assistant answers show headings, code and
//! quotes. Two line states
//! coexist: committed lines and an in-flight streaming line (the final
//! `commit` tick freezes the streaming text).
//!
//! [`LinesView`] is a thin renderer that writes already-reflowed
//! [`Line`]s into a ratatui [`Buffer`] with a scroll offset; it never
//! wraps, because wrapping happens in [`HistoryLine::display_lines`].

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Widget;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use tui_style::motion::MotionMode;

/// Role palette for a history line; the theme maps it to a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    /// Normal dialogue / assistant text.
    #[default]
    Default,
    /// Dimmed secondary text.
    Muted,
    /// Accent / brand emphasis.
    Accent,
    /// Additions (diff +, successes).
    Add,
    /// Removals (diff -).
    Remove,
    /// Warnings.
    Warning,
    /// Errors.
    Error,
    /// Highlights / selection.
    Highlight,
}

impl Role {
    /// Convert to the semantic ColorRole used by the theme.
    pub fn to_color_role(self) -> tui_style::theme::ColorRole {
        match self {
            Role::Default => tui_style::theme::ColorRole::Default,
            Role::Muted => tui_style::theme::ColorRole::Muted,
            Role::Accent => tui_style::theme::ColorRole::Accent,
            Role::Add => tui_style::theme::ColorRole::Add,
            Role::Remove => tui_style::theme::ColorRole::Remove,
            Role::Warning => tui_style::theme::ColorRole::Warning,
            Role::Error => tui_style::theme::ColorRole::Error,
            Role::Highlight => tui_style::theme::ColorRole::Highlight,
        }
    }
}

/// How a history line is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineState {
    /// A settled, committed line.
    #[default]
    Committed,
    /// An in-flight streaming line; a commit tick later freezes it.
    Streaming,
}

/// How a history line's source text becomes display rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineRender {
    /// Reflow the source text as-is: no Markdown interpretation.
    #[default]
    Plain,
    /// Interpret the source text as Markdown (headings, code, emphasis, block
    /// quotes) and reflow the styled result to the display width. The source
    /// text is kept verbatim for `source_text` (copy/search) so rendering
    /// never leaks back into the data.
    Markdown,
}

/// A scrollback entry holding reflowable source text.
///
/// Supports optional animation via [`MotionMode`]. When in animated mode,
/// streaming lines can apply shimmer effects to their content.
#[derive(Debug, Clone)]
pub struct HistoryLine {
    pub state: LineState,
    pub role: Role,
    render: LineRender,
    text: Text<'static>,
    motion_mode: MotionMode,
    /// Start moment (injectable clock ms) for shimmer timing.
    animation_start_ms: Option<u64>,
    /// Absolute prompt sequence number for history continuity checks.
    /// `0` means unsequenced (ephemeral rows); real history assigns
    /// monotonically increasing numbers so upper-splice paths can verify
    /// that prepended pages continue the numbering without gaps.
    seq_no: u64,
}

impl HistoryLine {
    /// New committed line from plain text.
    pub fn new(content: impl Into<String>) -> Self {
        Self::new_with_role(content, LineState::Committed, Role::Default)
    }

    /// New committed line with a role.
    pub fn new_role(content: impl Into<String>, role: Role) -> Self {
        Self::new_with_role(content, LineState::Committed, role)
    }

    /// New line with explicit state and role.
    pub fn new_with_role(content: impl Into<String>, state: LineState, role: Role) -> Self {
        Self {
            state,
            role,
            render: LineRender::Plain,
            text: Text::from(content.into()),
            motion_mode: MotionMode::default(),
            animation_start_ms: None,
            seq_no: 0,
        }
    }

    /// New committed assistant line rendered as Markdown.
    pub fn markdown(content: impl Into<String>) -> Self {
        Self::markdown_with(content, LineState::Committed, Role::Default)
    }

    /// New line rendered as Markdown with explicit state and role.
    pub fn markdown_with(content: impl Into<String>, state: LineState, role: Role) -> Self {
        let mut line = Self::new_with_role(content, state, role);
        line.render = LineRender::Markdown;
        line
    }

    /// New line with motion mode for animation support.
    pub fn with_motion_mode(
        content: impl Into<String>,
        state: LineState,
        role: Role,
        motion_mode: MotionMode,
    ) -> Self {
        Self {
            state,
            role,
            render: LineRender::Plain,
            text: Text::from(content.into()),
            motion_mode,
            animation_start_ms: if state == LineState::Streaming {
                Some(tui_clock::clock::now_ms())
            } else {
                None
            },
            seq_no: 0,
        }
    }

    /// Absolute sequence number (0 when unsequenced).
    pub fn seq_no(&self) -> u64 {
        self.seq_no
    }

    /// Assign the absolute sequence number, returning the line for chaining.
    pub fn with_seq_no(mut self, seq_no: u64) -> Self {
        self.seq_no = seq_no;
        self
    }

    /// Set the absolute sequence number in place.
    pub fn set_seq_no(&mut self, seq_no: u64) {
        self.seq_no = seq_no;
    }

    /// Set the motion mode for this line.
    pub fn set_motion_mode(&mut self, mode: MotionMode) {
        self.motion_mode = mode;
    }

    /// Get the current motion mode.
    pub fn motion_mode(&self) -> MotionMode {
        self.motion_mode
    }

    /// Start animation for streaming content.
    pub fn start_animation(&mut self) {
        if self.state == LineState::Streaming {
            self.animation_start_ms = Some(tui_clock::clock::now_ms());
        }
    }

    /// Stop animation (e.g., when line is committed).
    pub fn stop_animation(&mut self) {
        self.animation_start_ms = None;
    }

    /// Check if animation is active.
    pub fn is_animating(&self) -> bool {
        self.is_animating_at(tui_clock::clock::now_ms())
    }

    /// Check if animation is active at an explicit clock value. The state
    /// itself is clock-free; the instant is accepted so callers thread the
    /// injectable time source explicitly and stay deterministic.
    pub fn is_animating_at(&self, _now_ms: u64) -> bool {
        self.animation_start_ms.is_some() && self.motion_mode.should_animate()
    }

    /// Get animation tick for cache invalidation, discretized into buckets
    /// so the tick only changes the cache key once per bucket.
    pub fn animation_tick(&self) -> Option<u64> {
        self.animation_tick_at(tui_clock::clock::now_ms())
    }

    /// Animation tick for an explicit clock value (deterministic).
    pub fn animation_tick_at(&self, now_ms: u64) -> Option<u64> {
        if self.is_animating_at(now_ms) {
            Some(tui_clock::clock::anim_bucket(now_ms))
        } else {
            None
        }
    }

    /// Access the (ratatui) source text.
    pub fn text(&self) -> &Text<'static> {
        &self.text
    }

    /// Source-text view for copy paths: span contents joined per source
    /// line, without reflow or reordering. Copy always reads this, never
    /// the width-reflowed display rows.
    pub fn source_text(&self) -> String {
        self.text
            .lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Content identity for row-level caches: role and source text folded
    /// into one hash. Line state is intentionally excluded so a streaming
    /// line settling into its committed form hits the same cache entry
    /// instead of re-laying out identical text; sequence numbers are
    /// continuity metadata, not layout inputs, and stay out of the hash.
    /// Two lines sharing an identity lay out to the same display rows at
    /// the same width, so the preparation cache can reuse rows across
    /// replace, append and settle cycles.
    pub fn identity(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        std::mem::discriminant(&self.role).hash(&mut hasher);
        std::mem::discriminant(&self.render).hash(&mut hasher);
        for line in &self.text.lines {
            for span in &line.spans {
                span.content.as_ref().hash(&mut hasher);
                hasher.write_u8(0xff);
            }
            hasher.write_u8(0xfe);
        }
        hasher.finish()
    }

    /// Reflow the source text to `width` columns. Always returns at least
    /// one line per source line; empty content yields a single empty line.
    ///
    /// When animation is active for streaming content, applies shimmer
    /// effects to the rendered lines.
    pub fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.display_lines_at(width, tui_clock::clock::now_ms())
    }

    /// Reflow for an explicit clock value so animation frames stay
    /// deterministic under the injectable clock.
    pub fn display_lines_at(&self, width: u16, now_ms: u64) -> Vec<Line<'static>> {
        if self.render == LineRender::Markdown {
            return self.markdown_lines_at(width, now_ms);
        }
        let w = usize::from(width.max(1));
        let mut out = Vec::new();

        for line in &self.text.lines {
            if line.width() <= w {
                if self.is_animating_at(now_ms) {
                    // Apply shimmer animation to streaming content
                    let plain_text: String =
                        line.spans.iter().map(|s| s.content.as_ref()).collect();
                    let spans =
                        tui_style::motion::shimmer_text_at(&plain_text, self.motion_mode, now_ms);
                    out.push(Line::from(spans));
                } else {
                    out.push(own_line(line));
                }
            } else if self.is_animating_at(now_ms) {
                // Apply shimmer to wrapped lines
                for wrapped_line in wrap_line(line, w) {
                    let plain_text: String = wrapped_line
                        .spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect();
                    let spans =
                        tui_style::motion::shimmer_text_at(&plain_text, self.motion_mode, now_ms);
                    out.push(Line::from(spans));
                }
            } else {
                out.extend(wrap_line(line, w));
            }
        }

        if out.is_empty() {
            out.push(Line::from(""));
        }
        out
    }

    /// Render the Markdown source ([`HistoryLine::source_text`]) with the
    /// shared styled renderer, then reflow the styled rows to `width` so the
    /// result is a drop-in replacement for the plain reflow path. Shimmer is
    /// applied only while the line is animating, matching the plain path.
    fn markdown_lines_at(&self, width: u16, now_ms: u64) -> Vec<Line<'static>> {
        let w = usize::from(width.max(1));
        let motion = if self.is_animating_at(now_ms) {
            self.motion_mode
        } else {
            MotionMode::Static
        };
        let rendered = tui_markdown::markdown::styled::render_styled_lines_animated(
            &self.source_text(),
            u16::try_from(w).unwrap_or(u16::MAX),
            motion,
        );
        let mut out = Vec::new();
        for line in &rendered {
            if line.width() <= w {
                out.push(own_line(line));
            } else {
                out.extend(wrap_line(line, w));
            }
        }
        if out.is_empty() {
            out.push(Line::from(""));
        }
        out
    }

    /// Number of viewport rows this line occupies at `width`.
    pub fn desired_height(&self, width: u16) -> u16 {
        u16::try_from(self.display_lines(width).len()).unwrap_or(u16::MAX)
    }

    /// Plain-text view of the reflowed rows at `width`: same wrapping as
    /// [`HistoryLine::display_lines`] but copy-friendly `String` rows without
    /// styling. Used by the TUI transcript-window snapshot / common-prefix
    /// diff and shared by
    /// future copy / transcript / export paths.
    pub fn raw_lines(&self, width: u16) -> Vec<String> {
        self.display_lines(width)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }
}

/// Clone a borrowed line into an owned (static) line.
fn own_line(line: &Line<'_>) -> Line<'static> {
    let spans: Vec<Span<'static>> = line
        .spans
        .iter()
        .map(|s| Span::styled(s.content.to_string(), s.style))
        .collect();
    Line::from(spans)
}

/// Wrap a single line at `width` columns, honouring grapheme boundaries so
/// CJK and wide glyphs are not split mid-codepoint.
fn wrap_line(line: &Line<'_>, width: usize) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut cur: Vec<Span<'static>> = Vec::new();
    let mut cur_w = 0usize;

    for span in &line.spans {
        let style = span.style;
        let content: String = span.content.to_string();
        for grapheme in content.graphemes(true) {
            let gw = grapheme.width();
            // A grapheme wider than the whole column still gets emitted
            // (width-0 combining runs attach to the current line).
            if !cur.is_empty() && cur_w + gw > width {
                out.push(Line::from(std::mem::take(&mut cur)));
                cur_w = 0;
            }
            cur.push(Span::styled(grapheme.to_string(), style));
            cur_w += gw;
        }
    }
    if !cur.is_empty() {
        out.push(Line::from(cur));
    }
    if out.is_empty() {
        out.push(Line::from(""));
    }
    out
}

/// Thin renderer: writes already-reflowed lines into a [`Buffer`] with a
/// scroll offset. Never wraps (that is `HistoryLine::display_lines`'s job).
#[derive(Debug, Clone)]
pub struct LinesView<'a> {
    pub lines: &'a [Line<'a>],
    pub scroll_offset: u16,
}

impl<'a> LinesView<'a> {
    pub fn new(lines: &'a [Line<'a>]) -> Self {
        Self {
            lines,
            scroll_offset: 0,
        }
    }

    pub fn scroll(mut self, offset: u16) -> Self {
        self.scroll_offset = offset;
        self
    }
}

impl Widget for LinesView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let width = usize::from(area.width);
        let mut y = 0u16;
        for (i, line) in self.lines.iter().enumerate() {
            if (i as u16) < self.scroll_offset {
                continue;
            }
            if y >= area.height {
                break;
            }
            let row = area.y + y;
            render_row(buf, area.x, row, width, line);
            y += 1;
        }
    }
}

/// Write one reflowed line into a buffer row, clearing the row first and
/// clipping to the area width.
fn render_row(buf: &mut Buffer, x0: u16, row: u16, width: usize, line: &Line<'_>) {
    let clear: String = " ".repeat(width);
    buf.set_string(x0, row, &clear, Style::default());

    let mut col = 0usize;
    for span in &line.spans {
        if col >= width {
            break;
        }
        let avail = width - col;
        let content = span.content.as_ref();
        // Width-aware clip: take graphemes until `avail` columns are used.
        let mut take_chars = 0usize;
        let mut take_w = 0usize;
        for g in content.graphemes(true) {
            let gw = g.width();
            if take_w + gw > avail && take_chars > 0 {
                break;
            }
            take_w += gw;
            take_chars += 1;
        }
        let clipped: String = content.graphemes(true).take(take_chars).collect();
        buf.set_string(
            x0 + u16::try_from(col).unwrap_or(u16::MAX),
            row,
            &clipped,
            span.style,
        );
        col += clipped.width();
    }
}

/// Plain-text rendering of lines (tests / a pure-text headless path).
pub fn lines_to_string(lines: &[Line<'_>]) -> String {
    lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflow_splits_narrow_lines() {
        let h = HistoryLine::new("The quick brown fox jumps over the lazy dog guards the crown");
        let wide = h.display_lines(50);
        assert_eq!(wide.len(), 2, "50 cols keeps it on one long line");
        let narrow = h.display_lines(10);
        assert!(narrow.len() > wide.len(), "narrower width => more rows");
        // Wrapping never drops text: joining reflowed rows equals the source.
        let mut joined = String::new();
        let mut first = true;
        for line in &narrow {
            if !first {
                joined.push(' ');
            }
            first = false;
            joined.push_str(&lines_to_string(std::slice::from_ref(line)));
        }
        // Reflow keeps graphemes; spaces may redistribute, so just check
        // every original word remains present.
        for word in [
            "quick", "brown", "fox", "jumps", "over", "lazy", "guards", "crown",
        ] {
            assert!(joined.contains(word), "missing word {word}: {joined}");
        }
    }

    #[test]
    fn wide_glyphs_are_not_split() {
        let h = HistoryLine::new_with_role("你好世界", LineState::Committed, Role::Default);
        // Each CJK char is 2 cols, so width 4 fits exactly two chars/row.
        let lines = h.display_lines(4);
        let rendered = lines_to_string(&lines);
        assert_eq!(rendered, "你好\n世界", "reflow must not split a glyph");
    }

    #[test]
    fn desired_height_matches_line_count() {
        let h = HistoryLine::new("one two three four five six seven eight");
        for width in [5u16, 8, 12, 30, 60] {
            let rows = h.display_lines(width).len();
            assert_eq!(
                u16::try_from(rows).unwrap(),
                h.desired_height(width),
                "desired_height at width {width}"
            );
        }
    }

    #[test]
    fn empty_line_yields_one_row() {
        let h = HistoryLine::new("");
        assert_eq!(h.desired_height(10), 1);
        assert_eq!(lines_to_string(&h.display_lines(10)), "");
    }

    #[test]
    fn source_text_joins_spans_without_reflow() {
        use ratatui::text::Span;
        let mut text = Text::from("aaa bbb ccc");
        text.lines
            .push(Line::from(vec![Span::raw("x"), Span::raw("y")]));
        let h = HistoryLine {
            state: LineState::Committed,
            role: Role::Default,
            render: LineRender::Plain,
            text,
            motion_mode: MotionMode::default(),
            animation_start_ms: None,
            seq_no: 0,
        };
        assert_eq!(h.source_text(), "aaa bbb ccc\nxy");
        // Narrow reflow splits rows, the source stays whole.
        assert!(h.display_lines(4).len() > 2);
        assert_eq!(HistoryLine::new("one\ntwo").source_text(), "one\ntwo");
    }

    #[test]
    fn raw_lines_match_display_lines_as_plain_text() {
        let h =
            HistoryLine::new_with_role("你好世界 hello world", LineState::Committed, Role::Accent);
        for width in [4u16, 7, 12, 40] {
            let raw = h.raw_lines(width);
            assert_eq!(
                raw.join("\n"),
                lines_to_string(&h.display_lines(width)),
                "raw_lines must equal display_lines minus styling at width {width}"
            );
            for row in &raw {
                assert!(
                    row.width() <= width.max(1) as usize,
                    "reflowed row must fit width {width}: {row:?}"
                );
            }
        }
    }

    #[test]
    fn raw_lines_preserve_grapheme_boundaries() {
        let h = HistoryLine::new("你好世界");
        let raw = h.raw_lines(4);
        assert_eq!(raw, vec!["你好".to_string(), "世界".to_string()]);
    }

    #[test]
    fn markdown_lines_strip_inline_markup_and_keep_source() {
        let h = HistoryLine::markdown("**bold** and `code`");
        assert_eq!(
            lines_to_string(&h.display_lines(80)),
            "bold and code",
            "markdown lines render inline markup"
        );
        // The displayed form never leaks into the stored source (copy/search).
        assert_eq!(h.source_text(), "**bold** and `code`");
    }

    #[test]
    fn markdown_quote_marker_prefixes_each_quoted_line() {
        let h = HistoryLine::markdown("> quoted note\n> continued line\n");
        assert_eq!(
            lines_to_string(&h.display_lines(80)),
            "> quoted note\n> continued line"
        );
    }

    #[test]
    fn markdown_lines_stay_width_reflowed() {
        let h = HistoryLine::markdown("This is a long markdown sentence that must wrap.");
        for width in [8u16, 12, 30] {
            for row in &h.raw_lines(width) {
                assert!(
                    row.width() <= width.max(1) as usize,
                    "markdown row must fit width {width}: {row:?}"
                );
            }
        }
    }

    #[test]
    fn plain_lines_keep_markup_verbatim() {
        let h = HistoryLine::new("**bold** and `code`");
        assert_eq!(lines_to_string(&h.display_lines(80)), "**bold** and `code`");
    }

    #[test]
    fn markdown_line_renders_every_block_kind() {
        let src = "# Title\n\nA **bold** line.\n\n> note\n\n```\ncode()\n```";
        let h = HistoryLine::markdown(src);
        assert_eq!(
            lines_to_string(&h.display_lines(80)),
            "# Title\nA bold line.\n> note\n```\ncode()\n```"
        );
        // The stored source is untouched by rendering.
        assert_eq!(h.source_text(), src);
    }

    #[test]
    fn streaming_and_committed_states() {
        assert_eq!(HistoryLine::new("hi").state, LineState::Committed);
        let streaming = HistoryLine::new_with_role("stream…", LineState::Streaming, Role::Muted);
        assert_eq!(streaming.state, LineState::Streaming);
        assert_eq!(streaming.role, Role::Muted);
    }

    #[test]
    fn reflow_width_shapes_are_stable() {
        // In-memory golden values (no file writer in lib tests). The full
        // rendered files are (re)generated by the `component_output` example
        // into `crates/app/cli/wf-cli-demo/outputs/`.
        let h = HistoryLine::new_with_role(
            "  check inspect it code everywhere seat output",
            LineState::Committed,
            Role::Accent,
        );
        assert_eq!(
            lines_to_string(&h.display_lines(12)),
            "  check insp\nect it code \neverywhere s\neat output"
        );
        assert_eq!(
            lines_to_string(&h.display_lines(40)),
            "  check inspect it code everywhere seat \noutput"
        );
    }

    #[test]
    fn lines_view_writes_into_buffer() {
        use ratatui::style::Color;
        let content = vec![
            Line::from("row-a"),
            Line::from(vec![Span::styled("row-b", Style::default().fg(Color::Red))]),
        ];
        let mut buf = Buffer::empty(Rect::new(0, 0, 8, 2));
        LinesView::new(&content)
            .scroll(0)
            .render(Rect::new(0, 0, 8, 2), &mut buf);
        assert_eq!(
            buf[(0, 0)].symbol(),
            "r",
            "first glyph of first row rendered"
        );
        assert_eq!(buf[(4, 0)].symbol(), "a");
        assert_eq!(buf[(0, 1)].symbol(), "r");
        // The scroll drops the first row.
        let mut buf2 = Buffer::empty(Rect::new(0, 0, 8, 1));
        LinesView::new(&content)
            .scroll(1)
            .render(Rect::new(0, 0, 8, 1), &mut buf2);
        assert_eq!(buf2[(0, 0)].symbol(), "r", "scrolled past row-a");
        assert_eq!(buf2[(4, 0)].symbol(), "b");
    }

    #[test]
    fn own_clone_of_style_carries_style() {
        use ratatui::style::{Color, Modifier};
        let styled = Span::styled(
            "ok",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
        let line = Line::from(vec![styled]);
        let owned = own_line(&line);
        assert_eq!(owned.spans[0].style.fg, Some(Color::Cyan));
        assert_eq!(lines_to_string(&[owned]), "ok");
    }
}
