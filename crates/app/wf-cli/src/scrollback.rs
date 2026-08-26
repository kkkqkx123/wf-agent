//! Scrollback primitives shared by the mini footer and the full TUI.
//!
//! The core type is [`HistoryLine`]: it holds the **source text** (never a
//! width-fixed render cache) so that `display_lines(width)` recomputes the
//! line wrapping for the current terminal width on every call — the
//! resize-reflow ground rule. Two line states
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

/// How a history line is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineState {
    /// A settled, committed line.
    #[default]
    Committed,
    /// An in-flight streaming line; a commit tick later freezes it.
    Streaming,
}

/// A scrollback entry holding reflowable source text.
#[derive(Debug, Clone)]
pub struct HistoryLine {
    pub state: LineState,
    pub role: Role,
    text: Text<'static>,
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
            text: Text::from(content.into()),
        }
    }

    /// Access the (ratatui) source text.
    pub fn text(&self) -> &Text<'static> {
        &self.text
    }

    /// Reflow the source text to `width` columns. Always returns at least
    /// one line per source line; empty content yields a single empty line.
    pub fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let w = usize::from(width.max(1));
        let mut out = Vec::new();
        for line in &self.text.lines {
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
    /// styling. Used by the mini scrollback-window snapshot / common-prefix
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
        // into `crates/wf-cli/outputs/`.
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
