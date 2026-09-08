//! Modular bottom pane: layout calculation and component composition.
//!
//! The bottom pane is the footer area of the TUI, containing:
//! - [`Composer`] for text input
//! - [`StatusLine`] for agent status display
//! - [`ApprovalView`] for tool approval
//! - [`QuestionView`] for follow-up questions
//! - Selection popup panels (command palette, model picker, etc.)
//!
//! This module provides layout constants and calculation utilities shared
//! by all bottom pane components.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::footer::{FOOTER_BASE_HEIGHT, COMPOSER_MAIN_HEIGHT, PANEL_MAIN_HEIGHT,
    PERMISSION_MAIN_HEIGHT, QUESTION_MAIN_HEIGHT};

/// Layout configuration for the bottom pane.
#[derive(Debug, Clone)]
pub struct BottomPaneLayout {
    /// Total height of the bottom pane (including decorations).
    pub total_height: u16,
    /// Height of the top decoration row.
    pub top_decoration: u16,
    /// Height of the main content area.
    pub main_height: u16,
    /// Height of the status line.
    pub status_line: u16,
    /// Height of the bottom decoration row.
    pub bottom_decoration: u16,
}

impl BottomPaneLayout {
    /// Calculate layout for the given view/route combination.
    pub fn for_view(view: crate::footer::FooterView, route: crate::footer::FooterRoute) -> Self {
        let main = match (view, route) {
            (crate::footer::FooterView::Prompt, crate::footer::FooterRoute::Composer) => {
                COMPOSER_MAIN_HEIGHT
            }
            (crate::footer::FooterView::Prompt, _) => PANEL_MAIN_HEIGHT,
            (crate::footer::FooterView::Permission, _) => PERMISSION_MAIN_HEIGHT,
            (crate::footer::FooterView::Question, _) => QUESTION_MAIN_HEIGHT,
        };
        Self {
            total_height: FOOTER_BASE_HEIGHT + main,
            top_decoration: 1,
            main_height: main,
            status_line: 1,
            bottom_decoration: 1,
        }
    }

    /// Split an area into [top_decoration, main, status_line, bottom_decoration].
    pub fn split_area(&self, area: Rect) -> [Rect; 4] {
        let height = self.total_height.min(area.height);
        let area = Rect { height, ..area };
        let [top, main, status, bottom] = Layout::vertical([
            Constraint::Length(self.top_decoration),
            Constraint::Length(self.main_height),
            Constraint::Length(self.status_line),
            Constraint::Length(self.bottom_decoration),
        ])
        .areas(area);
        [top, main, status, bottom]
    }
}

/// Split an area vertically into main content and bottom pane.
pub fn split_main_and_footer(area: Rect, footer_height: u16) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(footer_height)])
        .split(area);
    (chunks[0], chunks[1])
}

/// Split the full TUI area into [content, notice_line].
pub fn split_content_and_notice(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);
    (chunks[0], chunks[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footer::{Footer, FooterView, FooterRoute};

    #[test]
    fn layout_calculates_correctly() {
        let layout = BottomPaneLayout::for_view(FooterView::Prompt, FooterRoute::Composer);
        assert_eq!(layout.total_height, FOOTER_BASE_HEIGHT + COMPOSER_MAIN_HEIGHT);
        assert_eq!(layout.main_height, COMPOSER_MAIN_HEIGHT);
        assert_eq!(layout.top_decoration, 1);
        assert_eq!(layout.status_line, 1);
        assert_eq!(layout.bottom_decoration, 1);
    }

    #[test]
    fn layout_permission_view() {
        let layout = BottomPaneLayout::for_view(FooterView::Permission, FooterRoute::Composer);
        assert_eq!(
            layout.total_height,
            FOOTER_BASE_HEIGHT + PERMISSION_MAIN_HEIGHT
        );
    }

    #[test]
    fn split_area_respects_bounds() {
        let layout = BottomPaneLayout::for_view(FooterView::Prompt, FooterRoute::Composer);
        let area = Rect::new(0, 0, 80, 5);
        let [top, main, status, bottom] = layout.split_area(area);
        // Area is smaller than total_height, so everything is compressed.
        assert!(top.height + main.height + status.height + bottom.height <= 5);
    }

    #[test]
    fn footer_height_matches_layout() {
        let footer = Footer::new();
        let layout = BottomPaneLayout::for_view(footer.view, footer.route);
        assert_eq!(footer.apply_height(), layout.total_height);
    }
}
