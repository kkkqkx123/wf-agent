//! Textual tab strip helpers shared by management screens, plus the chat
//! session tab strip drawn above the interactive scrollback.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;
use tui_style::theme::Theme;
use unicode_width::UnicodeWidthStr;

/// Render a textual tab strip. The active label is wrapped in brackets.
pub fn format_tabs(current: &str, labels: &[&str]) -> String {
    labels
        .iter()
        .map(|label| {
            if *label == current {
                format!("[{label}]")
            } else {
                (*label).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_tab_is_bracketed() {
        assert_eq!(format_tabs("B", &["A", "B", "C"]), "A [B] C");
    }

    #[test]
    fn missing_active_stays_plain() {
        assert_eq!(format_tabs("Z", &["A", "B"]), "A B");
    }

    #[test]
    fn session_cells_number_stream_and_truncate() {
        let tabs = vec![
            SessionTabItem::new("exec-alp...", true),
            SessionTabItem::new("exec-bet...", false),
        ];
        assert_eq!(
            session_tab_cells(&tabs, 24),
            vec![
                "1 exec-alp... \u{2026}".to_string(),
                "2 exec-bet...".to_string()
            ]
        );
        assert_eq!(
            session_tab_cells(&tabs, 4),
            vec![
                "1 exe\u{2026} \u{2026}".to_string(),
                "2 exe\u{2026}".to_string()
            ]
        );
    }

    #[test]
    fn session_cells_marks_pinned_tabs() {
        let tabs = vec![SessionTabItem::pinned("alpha", false)];
        assert_eq!(
            session_tab_cells(&tabs, 24),
            vec![format!("1 {PINNED_MARKER}alpha")]
        );
    }

    #[test]
    fn truncate_title_clips_with_ellipsis() {
        assert_eq!(truncate_title("abcdef", 6), "abcdef");
        assert_eq!(truncate_title("abcdef", 4), "abc\u{2026}");
        assert_eq!(truncate_title("abcdef", 1), "\u{2026}");
        assert_eq!(truncate_title("abcdef", 0), "");
    }

    #[test]
    fn strip_layout_shares_cells_close_and_plus() {
        let tabs = vec![
            SessionTabItem::new("alpha", false),
            SessionTabItem::new("beta", false),
        ];
        let area = Rect::new(0, 0, 40, 1);
        let layout = layout_tab_strip(area, &tabs, 0);
        assert_eq!(layout.segments.len(), 2);
        assert_eq!(layout.segments[0].index, 0);
        assert_eq!(layout.segments[0].x, 0);
        assert!(layout.segments[0].close_x.is_some());
        assert!(layout.segments[1].close_x.is_none());
        assert!(layout.plus.is_some());
        assert_eq!(tab_strip_hit(&layout, 0, 0, 0), Some(TabStripHit::Tab(0)));
        let close_x = layout.segments[0].close_x.expect("close mark");
        assert_eq!(
            tab_strip_hit(&layout, 0, close_x, 0),
            Some(TabStripHit::CloseTab(0))
        );
        let (plus_x, _) = layout.plus.expect("plus cell");
        assert_eq!(
            tab_strip_hit(&layout, 0, plus_x, 0),
            Some(TabStripHit::NewTab)
        );
        assert_eq!(tab_strip_hit(&layout, 0, 0, 1), None);
        assert_eq!(tab_strip_hit(&layout, 0, 39, 0), None);
    }

    #[test]
    fn strip_layout_drops_plus_when_narrow() {
        let tabs = vec![
            SessionTabItem::new("alpha", false),
            SessionTabItem::new("beta", false),
        ];
        let narrow = Rect::new(0, 0, 12, 1);
        let layout = layout_tab_strip(narrow, &tabs, 0);
        assert!(layout.plus.is_none());
    }
}

/// One chat session tab for the strip: title plus whether it is streaming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTabItem {
    pub title: String,
    pub streaming: bool,
    pub pinned: bool,
}

impl SessionTabItem {
    pub fn new(title: impl Into<String>, streaming: bool) -> Self {
        Self {
            title: title.into(),
            streaming,
            pinned: false,
        }
    }

    pub fn pinned(title: impl Into<String>, streaming: bool) -> Self {
        Self {
            title: title.into(),
            streaming,
            pinned: true,
        }
    }
}

/// Streaming marker appended to a streaming tab's title.
pub const STREAMING_MARKER: &str = " \u{2026}";

/// Pinned marker prefixed to a pinned tab's cell.
pub const PINNED_MARKER: &str = "◆";

/// Build the plain-text cells for the session tab strip: `n [◆]title[ …]`
/// per tab. Pure so the truncation policy is unit-testable without a
/// terminal.
pub fn session_tab_cells(tabs: &[SessionTabItem], max_title_width: usize) -> Vec<String> {
    tabs.iter()
        .enumerate()
        .map(|(index, tab)| {
            let title = truncate_title(&tab.title, max_title_width);
            let pin = if tab.pinned { PINNED_MARKER } else { "" };
            let marker = if tab.streaming { STREAMING_MARKER } else { "" };
            format!("{} {pin}{title}{marker}", index + 1)
        })
        .collect()
}

/// Truncate a title to a display width, appending an ellipsis when clipped.
pub fn truncate_title(title: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if title.width() <= max_width {
        return title.to_string();
    }
    if max_width == 1 {
        return "\u{2026}".to_string();
    }
    let mut width = 0usize;
    let mut out = String::new();
    for grapheme in unicode_segmentation::UnicodeSegmentation::graphemes(title, true) {
        let grapheme_width = unicode_width::UnicodeWidthStr::width(grapheme);
        if width + grapheme_width + 1 > max_width {
            break;
        }
        out.push_str(grapheme);
        width += grapheme_width;
    }
    out.push('\u{2026}');
    out
}

/// Trailing close mark inside the active tab cell.
pub const CLOSE_TAB_MARK: &str = "×";
/// Trailing new-tab cell text.
pub const NEW_TAB_CELL: &str = "+";

/// One laid-out tab strip segment: the tab index plus its absolute
/// horizontal span. `close_x` is the close mark cell of the active tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabStripSegment {
    pub index: usize,
    pub x: u16,
    pub width: u16,
    pub close_x: Option<u16>,
}

/// Shared tab strip geometry: paint and hit testing build from this one
/// function so clicks always land where the cells are drawn.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TabStripLayout {
    pub segments: Vec<TabStripSegment>,
    /// New-tab cell span `(x, width)` when trailing room allows one.
    pub plus: Option<(u16, u16)>,
}

/// Per-title width budget for a strip width and tab count.
pub fn tab_per_title(area_width: u16, len: usize) -> usize {
    usize::from(area_width)
        .saturating_sub(len * 4)
        .checked_div(len.max(1))
        .unwrap_or(0)
        .clamp(4, 24)
}

/// Lay out the tab strip left to right: one padded segment per cell with
/// a separator between segments, a close mark inside the active cell and
/// a new-tab cell when the trailing room fits it. Segments that do not
/// fit are dropped so no hit lands on truncated paint.
pub fn layout_tab_strip(area: Rect, tabs: &[SessionTabItem], active: usize) -> TabStripLayout {
    let mut layout = TabStripLayout::default();
    if area.height == 0 || area.width == 0 || tabs.is_empty() {
        return layout;
    }
    let end = area.x.saturating_add(area.width);
    let cells = session_tab_cells(tabs, tab_per_title(area.width, tabs.len()));
    let mut cursor = area.x;
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            cursor = cursor.saturating_add(1);
        }
        let mut text = cell.clone();
        let mut close_x = None;
        if index == active {
            close_x = Some(
                cursor
                    .saturating_add(1)
                    .saturating_add(text.width() as u16)
                    .saturating_add(1),
            );
            text.push(' ');
            text.push_str(CLOSE_TAB_MARK);
        }
        let width = (text.width() as u16).saturating_add(2);
        if cursor.saturating_add(width) > end {
            break;
        }
        layout.segments.push(TabStripSegment {
            index,
            x: cursor,
            width,
            close_x,
        });
        cursor = cursor.saturating_add(width);
    }
    if cursor.saturating_add(4) <= end {
        layout.plus = Some((cursor.saturating_add(1), 3));
    }
    layout
}

/// Pointer hit inside one tab strip layout. Hits outside every segment
/// and the new-tab cell stay `None` so empty strip room never switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStripHit {
    Tab(usize),
    CloseTab(usize),
    NewTab,
}

/// Map an absolute terminal cell to a tab strip hit. Only the strip row
/// participates; other rows never land.
pub fn tab_strip_hit(
    layout: &TabStripLayout,
    area_y: u16,
    col: u16,
    row: u16,
) -> Option<TabStripHit> {
    if row != area_y {
        return None;
    }
    for segment in &layout.segments {
        if col >= segment.x && col < segment.x.saturating_add(segment.width) {
            if segment.close_x == Some(col) {
                return Some(TabStripHit::CloseTab(segment.index));
            }
            return Some(TabStripHit::Tab(segment.index));
        }
    }
    if layout
        .plus
        .is_some_and(|(x, width)| col >= x && col < x.saturating_add(width))
    {
        return Some(TabStripHit::NewTab);
    }
    None
}

/// Draw the chat session tab strip into a one-row area. The active tab is
/// highlighted and carries a close mark; streaming tabs carry an ellipsis
/// marker and pinned tabs carry a diamond prefix. Segments lay out left to
/// right from the shared layout function and a new-tab cell follows when
/// the trailing room fits it.
pub fn draw_session_tab_bar(
    frame: &mut Frame,
    area: Rect,
    tabs: &[SessionTabItem],
    active: usize,
    theme: &Theme,
) {
    if area.height == 0 || area.width == 0 || tabs.is_empty() {
        return;
    }
    let layout = layout_tab_strip(area, tabs, active);
    let cells = session_tab_cells(tabs, tab_per_title(area.width, tabs.len()));
    let muted = Style::default().fg(Color::Rgb(theme.muted.r, theme.muted.g, theme.muted.b));
    let mut spans = Vec::with_capacity(tabs.len() * 2 + 2);
    for segment in &layout.segments {
        let Some(cell) = cells.get(segment.index) else {
            continue;
        };
        let is_active = segment.index == active;
        let pinned = tabs.get(segment.index).is_some_and(|tab| tab.pinned);
        let style = if is_active {
            Style::default()
                .fg(Color::Rgb(
                    theme.highlight.r,
                    theme.highlight.g,
                    theme.highlight.b,
                ))
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else if pinned {
            muted.add_modifier(Modifier::BOLD)
        } else {
            muted
        };
        if segment.index > 0 {
            spans.push(Span::styled("\u{2502}", muted));
        }
        if is_active {
            spans.push(Span::styled(format!(" {cell} {CLOSE_TAB_MARK} "), style));
        } else {
            spans.push(Span::styled(format!(" {cell} "), style));
        }
    }
    if layout.plus.is_some() {
        spans.push(Span::styled("\u{2502}", muted));
        spans.push(Span::styled(format!(" {NEW_TAB_CELL} "), muted));
    }
    let line = Line::from(spans);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(line),
        Rect::new(area.x, area.y, area.width, 1),
    );
}
