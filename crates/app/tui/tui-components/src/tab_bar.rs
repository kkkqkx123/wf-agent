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
    fn truncate_title_clips_with_ellipsis() {
        assert_eq!(truncate_title("abcdef", 6), "abcdef");
        assert_eq!(truncate_title("abcdef", 4), "abc\u{2026}");
        assert_eq!(truncate_title("abcdef", 1), "\u{2026}");
        assert_eq!(truncate_title("abcdef", 0), "");
    }
}

/// One chat session tab for the strip: title plus whether it is streaming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTabItem {
    pub title: String,
    pub streaming: bool,
}

impl SessionTabItem {
    pub fn new(title: impl Into<String>, streaming: bool) -> Self {
        Self {
            title: title.into(),
            streaming,
        }
    }
}

/// Streaming marker appended to a streaming tab's title.
pub const STREAMING_MARKER: &str = " \u{2026}";

/// Build the plain-text cells for the session tab strip: `n title[ …]` per
/// tab. Pure so the truncation policy is unit-testable without a terminal.
pub fn session_tab_cells(tabs: &[SessionTabItem], max_title_width: usize) -> Vec<String> {
    tabs.iter()
        .enumerate()
        .map(|(index, tab)| {
            let title = truncate_title(&tab.title, max_title_width);
            let marker = if tab.streaming { STREAMING_MARKER } else { "" };
            format!("{} {title}{marker}", index + 1)
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

/// Draw the chat session tab strip into a one-row area. The active tab is
/// highlighted; streaming tabs carry an ellipsis marker. Titles share the
/// row evenly and truncate when the row is narrow.
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
    let width = usize::from(area.width);
    let per_title = width
        .saturating_sub(tabs.len() * 4)
        .checked_div(tabs.len().max(1))
        .unwrap_or(0)
        .clamp(4, 24);
    let cells = session_tab_cells(tabs, per_title);
    let mut spans = Vec::with_capacity(tabs.len() * 2);
    for (index, cell) in cells.iter().enumerate() {
        let is_active = index == active;
        let style = if is_active {
            Style::default()
                .fg(Color::Rgb(
                    theme.highlight.r,
                    theme.highlight.g,
                    theme.highlight.b,
                ))
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(Color::Rgb(theme.muted.r, theme.muted.g, theme.muted.b))
        };
        if index > 0 {
            spans.push(Span::styled(
                "\u{2502}",
                Style::default().fg(Color::Rgb(theme.muted.r, theme.muted.g, theme.muted.b)),
            ));
        }
        spans.push(Span::styled(format!(" {cell} "), style));
    }
    let line = Line::from(spans);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(line),
        Rect::new(area.x, area.y, area.width, 1),
    );
}
