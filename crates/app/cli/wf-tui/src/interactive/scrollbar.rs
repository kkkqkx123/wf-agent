//! Scrollbar gutter geometry and thumb-drag interaction.

use ratatui::layout::Rect;

use crate::interactive::InteractiveController;
use tui_components::scrollbar;

impl InteractiveController {
    /// Whether the session scrollbar is visible for the given window.
    /// Combines the explicit switch with the shared overflow and narrow
    /// guards so hidden frames stay pixel-identical to the no-bar path.
    pub(super) fn scrollbar_visible_for(
        &self,
        full_area: Rect,
        total: usize,
        viewport: usize,
    ) -> bool {
        self.show_session_scrollbar && scrollbar::visible_in(full_area, total, viewport)
    }

    /// Whether the gutter is on screen right now, from the accurate total.
    pub(super) fn scrollbar_currently_visible(&self, full_area: Rect) -> bool {
        if self.scrollback.is_empty() && self.streaming.is_none() {
            return false;
        }
        let viewport = usize::from(full_area.height);
        let total = self.accurate_total_rows();
        self.scrollbar_visible_for(full_area, total, viewport) && full_area.width > 1
    }

    /// Gutter rectangle for the last published viewport, when the bar shows.
    /// Hidden switches and narrow terminals yield `None` so no click lands.
    pub(crate) fn session_gutter_rect(&self) -> Option<Rect> {
        let area = self.last_scroll_area?;
        if !self.scrollbar_currently_visible(area) {
            return None;
        }
        let (_, gutter) = scrollbar::split_content_gutter(area);
        if gutter.width == 0 || gutter.height == 0 {
            return None;
        }
        Some(gutter)
    }

    /// Live total, capacity and top for gutter math. `None` without a
    /// published viewport.
    fn gutter_window(&self) -> Option<(usize, usize, usize)> {
        let area = self.last_scroll_area?;
        let capacity = usize::from(area.height.max(1));
        let total = self.accurate_total_rows();
        if !self.scrollbar_visible_for(area, total, capacity) || area.width <= 1 {
            return None;
        }
        let top = scrollbar::top_for_tail(total, capacity, self.view_scroll);
        Some((total, capacity, top))
    }

    /// Whether a press row sits on the thumb. Used to enter drag versus
    /// track-click.
    pub(crate) fn gutter_press_on_thumb(&self, press_row: u16) -> bool {
        let Some(gutter) = self.session_gutter_rect() else {
            return false;
        };
        let Some((total, capacity, top)) = self.gutter_window() else {
            return false;
        };
        let relative = press_row.saturating_sub(gutter.y) as usize;
        scrollbar::thumb_contains(total, capacity, top, usize::from(gutter.height), relative)
    }

    /// Jump to a gutter press row by proportion. Returns true when the
    /// viewport moved.
    pub(crate) fn scroll_to_gutter_row(&mut self, press_row: u16) -> bool {
        let Some(gutter) = self.session_gutter_rect() else {
            return false;
        };
        let Some((total, capacity, _)) = self.gutter_window() else {
            return false;
        };
        let top = scrollbar::top_for_gutter_press(press_row, gutter, total, capacity);
        let offset = scrollbar::tail_offset_for_top(top, total, capacity);
        self.tail_lag = 0;
        self.soft_invalidate();
        self.view_scroll = offset;
        true
    }

    /// Whether a thumb drag is in progress.
    pub(crate) fn is_gutter_dragging(&self) -> bool {
        self.gutter_drag
    }

    /// Begin a thumb drag after a press on the thumb.
    pub(crate) fn begin_gutter_drag(&mut self) {
        self.gutter_drag = true;
    }

    /// Update a thumb drag to a new row. Reuses the track proportion with
    /// the live total so held drags track appends.
    pub(crate) fn update_gutter_drag(&mut self, row: u16) -> bool {
        if !self.gutter_drag {
            return false;
        }
        self.scroll_to_gutter_row(row)
    }

    /// End a thumb drag on release or cancel.
    pub(crate) fn end_gutter_drag(&mut self) {
        self.gutter_drag = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use tui_components::scrollbar;

    #[tokio::test]
    async fn scrollbar_switch_and_visibility_combos() {
        let client = wf_client::WfClient::new(wf_client::ClientConfig::new("http://127.0.0.1:1"));
        let mut on = InteractiveController::start(client.clone(), "exec".into()).await;
        let mut off = InteractiveController::start(client, "exec".into()).await;
        on.set_session_scrollbar(true);
        off.set_session_scrollbar(false);
        let wide = Rect::new(0, 0, 30, 10);
        let narrow = Rect::new(0, 0, scrollbar::MIN_AREA_WIDTH - 1, 10);
        let tiny = Rect::new(0, 0, 30, 2);
        assert!(on.scrollbar_visible_for(wide, 50, 10));
        assert!(!on.scrollbar_visible_for(wide, 5, 10));
        assert!(!on.scrollbar_visible_for(narrow, 50, 10));
        assert!(!on.scrollbar_visible_for(tiny, 50, 2));
        assert!(!off.scrollbar_visible_for(wide, 50, 10));
    }

    #[test]
    fn tail_conversion_includes_streaming_rows() {
        let cached = 90usize;
        let streaming = 10usize;
        let total = cached + streaming;
        let capacity = 10usize;
        assert_eq!(scrollbar::top_for_tail(total, capacity, 0), 90);
        assert_eq!(scrollbar::top_for_tail(total, capacity, 5), 85);
        assert_eq!(scrollbar::top_for_tail(total, capacity, usize::MAX), 0);
    }
}
