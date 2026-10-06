//! `Ctrl+T` history overlay: one session's scrollback, one page at a time.
//!
//! The overlay reads through the same cursor-paginated replay source the
//! interactive session uses ([`replay_scrollack_page`]): the tail page lands
//! when the overlay opens and a `PageUp` pinned to the oldest loaded row asks
//! for the page behind it. The window keeps the pages it has landed so a
//! later `PageUp` scrolls the rows already in memory before paging again.
//!
//! Timeline and node segments join this view once their contract exists.

use crate::pager::ReplayPager;
use crate::replay::ReplayPage;
use crate::screen_draw::HistoryOverlayView;
use crate::transcript::HistoryLine;

/// Display rows one `PageUp` / `PageDown` moves the viewport by.
pub const VIEWPORT_ROWS: usize = 10;

/// Loaded scrollback of one session plus the viewport riding on it.
#[derive(Debug, Default)]
pub struct HistoryOverlay {
    session_id: Option<String>,
    lines: Vec<HistoryLine>,
    pager: ReplayPager,
    scroll: usize,
    at_top: bool,
    loading: bool,
    error: Option<String>,
}

impl HistoryOverlay {
    /// Open a fresh window at the tail page of `session_id`, dropping whatever
    /// was loaded before: a window belongs to exactly one session.
    pub fn begin(&mut self, session_id: String) {
        self.session_id = Some(session_id);
        self.lines.clear();
        self.pager.begin();
        self.scroll = 0;
        self.at_top = false;
        self.loading = true;
        self.error = None;
    }

    /// Session whose scrollback the window holds.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Take the cursor of the page behind the loaded window and mark the
    /// fetch in flight. `None` while the viewport is not at the top, a fetch
    /// is already running, or the beginning of the session was reached.
    pub fn start_earlier(&mut self) -> Option<i64> {
        if !self.at_top || !self.pager.can_load_earlier() {
            return None;
        }
        let cursor = self.pager.cursor?;
        self.pager.start_earlier();
        self.loading = true;
        Some(cursor)
    }

    /// Fold a landed page into the window. The tail page replaces the window;
    /// an earlier page is prepended above everything already loaded, which
    /// leaves a tail-relative viewport offset untouched.
    pub fn land(&mut self, page: ReplayPage, earlier: bool) {
        self.loading = false;
        self.error = None;
        if earlier {
            self.pager.land_earlier(page.has_more, page.next_before);
            let ReplayPage { lines, .. } = page;
            self.lines.splice(0..0, lines);
        } else {
            self.pager.land_initial(page.has_more, page.next_before);
            self.lines = page.lines;
            self.scroll = 0;
            self.at_top = false;
        }
    }

    /// Stop paging after a failed fetch and keep the reason for the status
    /// line; the rows already loaded stay readable.
    pub fn fail(&mut self, error: String) {
        self.pager.fail();
        self.loading = false;
        self.error = Some(error);
    }

    /// Move the viewport one page towards the oldest loaded row.
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_add(VIEWPORT_ROWS);
    }

    /// Move the viewport one page towards the tail.
    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_sub(VIEWPORT_ROWS);
    }

    /// Pin the viewport to the window the last draw laid out: the renderer
    /// clamps the offset and reports whether it reached the oldest row.
    pub fn set_viewport(&mut self, scroll: usize, at_top: bool) {
        self.scroll = scroll;
        self.at_top = at_top;
    }

    /// Borrowed snapshot handed to the renderer.
    pub fn view(&self) -> HistoryOverlayView<'_> {
        HistoryOverlayView {
            session_id: self.session_id.as_deref(),
            lines: &self.lines,
            scroll: self.scroll,
            has_more: self.pager.is_partial(),
            loading: self.loading,
            error: self.error.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Role;

    fn page(lines: Vec<HistoryLine>, has_more: bool, next_before: Option<i64>) -> ReplayPage {
        ReplayPage {
            lines,
            next_before,
            has_more,
        }
    }

    #[test]
    fn tail_page_resets_the_viewport() {
        let mut overlay = HistoryOverlay::default();
        overlay.begin("exec-1".into());
        overlay.scroll_up();
        overlay.set_viewport(10, true);
        assert_eq!(overlay.session_id(), Some("exec-1"));

        overlay.land(
            page(
                vec![HistoryLine::new_role("tail", Role::Default)],
                true,
                Some(500),
            ),
            false,
        );
        assert_eq!(overlay.view().scroll, 0);
        assert!(overlay.view().has_more);
        assert!(!overlay.view().loading);
    }

    #[test]
    fn earlier_page_prepends_and_keeps_the_tail_offset() {
        let mut overlay = HistoryOverlay::default();
        overlay.begin("exec-1".into());
        overlay.land(
            page(
                vec![HistoryLine::new_role("newest", Role::Default)],
                true,
                Some(500),
            ),
            false,
        );
        overlay.set_viewport(10, true);

        // A fetch must be taken while the viewport sits on the oldest row.
        assert_eq!(overlay.start_earlier(), Some(500));
        // In flight: a second request would double-fetch the same page.
        assert_eq!(overlay.start_earlier(), None);
        assert!(overlay.view().loading);

        overlay.land(
            page(
                vec![HistoryLine::new_role("older", Role::Default)],
                true,
                Some(200),
            ),
            true,
        );
        let view = overlay.view();
        assert_eq!(view.lines.len(), 2);
        assert_eq!(view.lines[0].raw_lines(40), vec!["older".to_string()]);
        // The viewport counts rows above the tail, so prepending does not move it.
        assert_eq!(view.scroll, 10);
        assert!(view.has_more);
    }

    #[test]
    fn paging_stops_once_the_session_start_is_reached() {
        let mut overlay = HistoryOverlay::default();
        overlay.begin("exec-1".into());
        overlay.land(
            page(
                vec![HistoryLine::new_role("a", Role::Default)],
                true,
                Some(500),
            ),
            false,
        );
        overlay.set_viewport(10, true);
        assert_eq!(overlay.start_earlier(), Some(500));

        overlay.land(
            page(vec![HistoryLine::new_role("b", Role::Default)], false, None),
            true,
        );
        overlay.set_viewport(10, true);
        assert_eq!(overlay.start_earlier(), None);
        assert!(!overlay.view().has_more);
    }

    #[test]
    fn a_failed_fetch_keeps_the_loaded_rows() {
        let mut overlay = HistoryOverlay::default();
        overlay.begin("exec-1".into());
        overlay.land(
            page(
                vec![HistoryLine::new_role("kept", Role::Default)],
                false,
                None,
            ),
            false,
        );
        overlay.fail("network down".into());

        let view = overlay.view();
        assert_eq!(view.lines.len(), 1);
        assert_eq!(view.error, Some("network down"));
        assert!(!view.has_more);
        assert!(!view.loading);
    }

    #[test]
    fn scrolling_clamps_at_the_tail() {
        let mut overlay = HistoryOverlay::default();
        overlay.begin("exec-1".into());
        overlay.scroll_down();
        assert_eq!(overlay.view().scroll, 0);
    }
}
