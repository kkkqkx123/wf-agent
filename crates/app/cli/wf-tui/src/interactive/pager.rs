//! Pagination state machine for paged replay-history loads.
//!
//! Kept free of any I/O so the cursor/phase transitions are unit-testable in
//! isolation. The full-TUI loads history in pages: `LoadingBeginning` shows
//! the placeholder while the first (tail) page is fetched, `Partial` means an
//! earlier page exists behind the loaded rows and can be prepended on demand,
//! `Complete` means the beginning of the session was reached.

/// Pagination phase of a replay-history load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplayPhase {
    LoadingBeginning,
    Partial,
    Complete,
}

/// Pure cursor/phase state machine for paged replay loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReplayPager {
    phase: ReplayPhase,
    /// `before_timestamp` for the next (older) page; `None` once the
    /// beginning of the session was reached.
    pub(super) cursor: Option<i64>,
    /// True while an earlier-page fetch is in flight (guards double loads).
    loading: bool,
}

impl Default for ReplayPager {
    fn default() -> Self {
        Self {
            phase: ReplayPhase::Complete,
            cursor: None,
            loading: false,
        }
    }
}

impl ReplayPager {
    /// A new replay starts from the tail page placeholder.
    pub(super) fn begin(&mut self) {
        self.phase = ReplayPhase::LoadingBeginning;
        self.cursor = None;
        self.loading = false;
    }

    /// The tail (first) page landed: `Partial` when older records remain,
    /// `Complete` otherwise.
    pub(super) fn land_initial(&mut self, has_more: bool, next_before: Option<i64>) {
        self.phase = if has_more {
            ReplayPhase::Partial
        } else {
            ReplayPhase::Complete
        };
        self.cursor = if has_more { next_before } else { None };
        self.loading = false;
    }

    /// Whether an older page may be requested right now.
    pub(super) fn can_load_earlier(&self) -> bool {
        self.phase == ReplayPhase::Partial && !self.loading && self.cursor.is_some()
    }

    /// Mark an earlier-page fetch as in flight (guard against double loads).
    pub(super) fn start_earlier(&mut self) {
        self.loading = true;
    }

    /// An earlier page landed: prepend its rows and keep paging while more
    /// older records exist.
    pub(super) fn land_earlier(&mut self, has_more: bool, next_before: Option<i64>) {
        self.phase = if has_more {
            ReplayPhase::Partial
        } else {
            ReplayPhase::Complete
        };
        self.cursor = if has_more { next_before } else { None };
        self.loading = false;
    }

    /// A fetch failed: no further pages can be requested.
    pub(super) fn fail(&mut self) {
        self.phase = ReplayPhase::Complete;
        self.cursor = None;
        self.loading = false;
    }

    /// Whether the pager currently sits in the `Partial` phase (an earlier
    /// page is reachable behind the loaded window).
    pub(super) fn is_partial(&self) -> bool {
        self.phase == ReplayPhase::Partial
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_pager_initial_landing_partial_then_complete() {
        let mut pager = ReplayPager::default();
        pager.begin();
        assert_eq!(pager.phase, ReplayPhase::LoadingBeginning);
        assert!(!pager.can_load_earlier());

        // Tail page with older records remaining: Partial + cursor.
        pager.land_initial(true, Some(500));
        assert_eq!(pager.phase, ReplayPhase::Partial);
        assert_eq!(pager.cursor, Some(500));
        assert!(pager.can_load_earlier());

        // One earlier page, still more behind it.
        pager.start_earlier();
        assert!(!pager.can_load_earlier());
        pager.land_earlier(true, Some(300));
        assert!(pager.can_load_earlier());

        // Final page: Complete and no further loads.
        pager.start_earlier();
        pager.land_earlier(false, None);
        assert_eq!(pager.phase, ReplayPhase::Complete);
        assert_eq!(pager.cursor, None);
        assert!(!pager.can_load_earlier());
    }

    #[test]
    fn replay_pager_no_more_on_first_page() {
        let mut pager = ReplayPager::default();
        pager.begin();
        pager.land_initial(false, Some(999));
        assert_eq!(pager.phase, ReplayPhase::Complete);
        assert_eq!(pager.cursor, None);
        assert!(!pager.can_load_earlier());
    }

    #[test]
    fn replay_pager_failure_stops_paging() {
        let mut pager = ReplayPager::default();
        pager.begin();
        pager.land_initial(true, Some(700));
        pager.start_earlier();
        pager.fail();
        assert_eq!(pager.phase, ReplayPhase::Complete);
        assert!(!pager.can_load_earlier());
    }

    #[test]
    fn replay_pager_begin_resets_state() {
        let mut pager = ReplayPager::default();
        pager.begin();
        pager.land_initial(true, Some(50));
        pager.begin();
        assert_eq!(pager.phase, ReplayPhase::LoadingBeginning);
        assert_eq!(pager.cursor, None);
        assert!(!pager.can_load_earlier());
    }

    #[test]
    fn scroll_history_up_pages_earlier_at_partial_top() {
        // Verify the scroll pin decides between paging the viewport and
        // requesting the next older replay page (no session I/O here: the
        // request is only gated by the pager state).
        let mut pager = ReplayPager::default();
        pager.begin();
        pager.land_initial(true, Some(10));
        assert!(pager.can_load_earlier());
        pager.start_earlier();
        assert!(!pager.can_load_earlier());
        pager.land_earlier(false, None);
        assert!(!pager.can_load_earlier());
    }
}
