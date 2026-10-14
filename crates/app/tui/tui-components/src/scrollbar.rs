//! Shared vertical scrollbar for TUI panes.
//!
//! Thin wrapper over the `ratatui` scrollbar widget so every scrollable pane
//! shares one visibility rule and one position convention. The widget itself
//! is view-only: keys and the mouse wheel keep driving the existing scroll
//! offsets, this module only paints the position indicator.

use ratatui::layout::Rect;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;
use tui_style::theme::{ColorRole, Theme};

/// Minimum viewport height for a scrollbar to be useful.
pub const MIN_VIEWPORT_HEIGHT: usize = 3;

/// Full-area width below which the bar stays hidden to protect content.
pub const MIN_AREA_WIDTH: u16 = 20;

/// Whether a pane with `total_rows` behind a `viewport_rows` window needs a bar.
pub fn should_show(total_rows: usize, viewport_rows: usize) -> bool {
    viewport_rows >= MIN_VIEWPORT_HEIGHT && total_rows > viewport_rows
}

/// Visibility check including the narrow-terminal guard.
pub fn visible_in(area: Rect, total_rows: usize, viewport_rows: usize) -> bool {
    area.width >= MIN_AREA_WIDTH && should_show(total_rows, viewport_rows)
}

/// Clamp a top-anchored offset to its laid-out window.
pub fn clamp_top(position: usize, total_rows: usize, viewport_rows: usize) -> usize {
    position.min(total_rows.saturating_sub(viewport_rows))
}

/// Convert a bottom-anchored tail offset (rows above the tail) to a top index.
pub fn top_for_tail(total_rows: usize, viewport_rows: usize, rows_above_tail: usize) -> usize {
    let max = total_rows.saturating_sub(viewport_rows);
    max.saturating_sub(rows_above_tail.min(max))
}

/// Split a content area into the text area and its one-cell right gutter.
/// The caller must have checked visibility first.
pub fn split_content_gutter(area: Rect) -> (Rect, Rect) {
    if area.width <= 1 {
        return (area, Rect::new(area.x, area.y, 0, area.height));
    }
    let content = Rect::new(area.x, area.y, area.width - 1, area.height);
    let gutter = Rect::new(
        area.x.saturating_add(area.width - 1),
        area.y,
        1,
        area.height,
    );
    (content, gutter)
}

/// Paint a right-side vertical bar into `gutter` for a top-anchored window.
pub fn render(
    frame: &mut Frame,
    gutter: Rect,
    total_rows: usize,
    viewport_rows: usize,
    top_position: usize,
    theme: &Theme,
) {
    if gutter.width == 0 || gutter.height == 0 {
        return;
    }
    let top = clamp_top(top_position, total_rows, viewport_rows);
    let mut state = ScrollbarState::new(total_rows)
        .position(top)
        .viewport_content_length(viewport_rows);
    let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"))
        .thumb_symbol("▐")
        .thumb_style(theme.style_for_role(ColorRole::Muted))
        .track_style(theme.dim_style_for_role(ColorRole::Muted));
    frame.render_stateful_widget(bar, gutter, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::dark_default()
    }

    #[test]
    fn hidden_without_overflow_or_tiny_viewport() {
        assert!(!should_show(5, 10));
        assert!(!should_show(10, 10));
        assert!(!should_show(10, 2));
        assert!(should_show(11, 10));
    }

    #[test]
    fn narrow_areas_hide_the_bar() {
        let area = Rect::new(0, 0, MIN_AREA_WIDTH - 1, 10);
        assert!(!visible_in(area, 100, 10));
        let area = Rect::new(0, 0, MIN_AREA_WIDTH, 10);
        assert!(visible_in(area, 100, 10));
    }

    #[test]
    fn tail_offset_converts_to_top_index() {
        assert_eq!(top_for_tail(100, 10, 0), 90);
        assert_eq!(top_for_tail(100, 10, 5), 85);
        assert_eq!(top_for_tail(100, 10, usize::MAX), 0);
        assert_eq!(top_for_tail(5, 10, 0), 0);
    }

    #[test]
    fn split_reserves_one_right_cell() {
        let (content, gutter) = split_content_gutter(Rect::new(2, 3, 10, 5));
        assert_eq!((content.width, gutter.width), (9, 1));
        assert_eq!(gutter.x, 11);
        assert_eq!((gutter.y, gutter.height), (3, 5));
    }

    #[test]
    fn render_into_test_backend_keeps_text_area_intact() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| {
                let area = frame.area();
                let (content, gutter) = split_content_gutter(area);
                assert_eq!(content.width + gutter.width, area.width);
                render(frame, gutter, 100, 10, 0, &theme());
            })
            .expect("draw scrollbar");
    }
}
