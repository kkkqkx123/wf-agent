//! Session list browser state: selection over the open chat tabs plus a
//! read-only recent-sessions pane.
//!
//! The browser is transient and never enters the navigation stack. It owns
//! only the selected row so any screen, including the session screen, can
//! open it without leaving the current page. Rendering reads the live
//! [`crate::session_tabs::SessionTabs`] snapshot directly; recent sessions
//! arrive from a background fetch of the agent-loop summaries.
//!
//! The recent pane is read-only by design: Enter opens the selected session
//! as a replay tab (no server writes), matching the phase 2 scope of
//! read-then-revisit without delete or rename.

use tui_core::screen_data::short_id;

/// One recent session row from the agent-loop summaries.
#[derive(Debug, Clone)]
pub struct RecentSession {
    pub id: String,
    pub status: String,
    /// Pre-formatted relative age (`5m`, `2h`, `3d`).
    pub started: String,
}

impl RecentSession {
    /// Single display line for the recent pane.
    pub fn display(&self) -> String {
        format!(
            "{} · {} · {}",
            short_id(&self.id),
            self.status,
            self.started
        )
    }
}

/// Which half of the session list browser is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionListPane {
    /// Live tabs (existing behavior).
    #[default]
    Tabs,
    /// Recent sessions fetched from the server.
    Recent,
}

/// Relative age label for a start timestamp. Pure so it unit-tests without
/// a clock: `now_ms` is injected by the caller.
pub fn relative_age(start_ms: i64, now_ms: u64) -> String {
    if start_ms <= 0 {
        return "-".to_string();
    }
    let delta_ms = now_ms.saturating_sub(start_ms.max(0) as u64);
    let minutes = delta_ms / 60_000;
    if minutes < 1 {
        return "now".to_string();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days}d");
    }
    format!("{}mo", days / 30)
}

/// Session list selection state.
#[derive(Debug, Default)]
pub struct SessionList {
    selected: usize,
    pane: SessionListPane,
    recent: Vec<RecentSession>,
    recent_selected: usize,
    recent_id: Option<String>,
    recent_filter: String,
}

impl SessionList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn pane(&self) -> SessionListPane {
        self.pane
    }

    /// Clamp the selection into a list of `len` rows.
    pub fn open(&mut self, active: usize, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = active.min(len - 1);
        }
        self.pane = SessionListPane::Tabs;
    }

    pub fn select(&mut self, index: usize, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = index.min(len - 1);
        }
    }

    pub fn move_next(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    pub fn move_prev(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        if self.selected == 0 {
            self.selected = len - 1;
        } else {
            self.selected -= 1;
        }
    }

    /// Switch between the tabs and recent panes.
    pub fn toggle_pane(&mut self) {
        self.pane = match self.pane {
            SessionListPane::Tabs => SessionListPane::Recent,
            SessionListPane::Recent => SessionListPane::Tabs,
        };
    }

    /// Replace the recent rows (already newest-first from the fetch),
    /// keeping the selection on the same session id when listed.
    pub fn set_recent(&mut self, rows: Vec<RecentSession>) {
        self.recent = rows;
        self.restore_recent_selection();
    }

    pub fn recent_filter(&self) -> &str {
        &self.recent_filter
    }

    pub fn recent_selected(&self) -> usize {
        self.recent_selected
    }

    /// Recent rows matching the current substring filter (case-insensitive
    /// over id and status).
    pub fn recent_visible(&self) -> Vec<&RecentSession> {
        let needle = self.recent_filter.to_lowercase();
        if needle.trim().is_empty() {
            return self.recent.iter().collect();
        }
        self.recent
            .iter()
            .filter(|row| {
                row.id.to_lowercase().contains(&needle)
                    || row.status.to_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Selected recent session id, if any row is visible.
    pub fn recent_selected_id(&self) -> Option<String> {
        self.recent_visible()
            .get(self.recent_selected)
            .map(|row| row.id.clone())
    }

    pub fn recent_move_next(&mut self) {
        let len = self.recent_visible().len();
        if len == 0 {
            return;
        }
        self.recent_selected = (self.recent_selected + 1) % len;
        self.remember_recent();
    }

    pub fn recent_move_prev(&mut self) {
        let len = self.recent_visible().len();
        if len == 0 {
            return;
        }
        if self.recent_selected == 0 {
            self.recent_selected = len - 1;
        } else {
            self.recent_selected -= 1;
        }
        self.remember_recent();
    }

    /// Page the recent selection, clamping at the ends.
    pub fn recent_page_by(&mut self, delta: isize) {
        let len = self.recent_visible().len();
        if len == 0 {
            return;
        }
        let next = self.recent_selected as isize + delta;
        self.recent_selected = next.clamp(0, len as isize - 1) as usize;
        self.remember_recent();
    }

    pub fn recent_jump_to_first(&mut self) {
        if self.recent_visible().is_empty() {
            return;
        }
        self.recent_selected = 0;
        self.remember_recent();
    }

    pub fn recent_jump_to_last(&mut self) {
        let len = self.recent_visible().len();
        if len == 0 {
            return;
        }
        self.recent_selected = len - 1;
        self.remember_recent();
    }

    pub fn recent_filter_push(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.recent_filter.push(c);
        self.restore_recent_selection();
    }

    pub fn recent_filter_backspace(&mut self) {
        self.recent_filter.pop();
        self.restore_recent_selection();
    }

    pub fn clear_recent_filter(&mut self) {
        if self.recent_filter.is_empty() {
            return;
        }
        self.recent_filter.clear();
        self.restore_recent_selection();
    }

    fn remember_recent(&mut self) {
        self.recent_id = self.recent_selected_id();
    }

    /// Restore the recent selection after the rows or filter changed: same
    /// session id when still visible, else clamp into range.
    fn restore_recent_selection(&mut self) {
        let len = self.recent_visible().len();
        if len == 0 {
            self.recent_selected = 0;
            return;
        }
        if let Some(id) = self.recent_id.clone() {
            if let Some(pos) = self.recent_visible().iter().position(|row| row.id == id) {
                self.recent_selected = pos;
                return;
            }
        }
        self.recent_selected = self.recent_selected.min(len - 1);
        self.remember_recent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recent(id: &str) -> RecentSession {
        RecentSession {
            id: id.to_string(),
            status: "completed".to_string(),
            started: "5m".to_string(),
        }
    }

    #[test]
    fn open_clamps_active_into_range() {
        let mut list = SessionList::new();
        list.open(5, 3);
        assert_eq!(list.selected(), 2);
        list.open(1, 3);
        assert_eq!(list.selected(), 1);
    }

    #[test]
    fn navigation_wraps() {
        let mut list = SessionList::new();
        list.open(0, 3);
        list.move_prev(3);
        assert_eq!(list.selected(), 2);
        list.move_next(3);
        assert_eq!(list.selected(), 0);
        list.select(9, 3);
        assert_eq!(list.selected(), 2);
    }

    #[test]
    fn open_resets_to_tabs_pane() {
        let mut list = SessionList::new();
        list.toggle_pane();
        assert_eq!(list.pane(), SessionListPane::Recent);
        list.open(0, 1);
        assert_eq!(list.pane(), SessionListPane::Tabs);
    }

    #[test]
    fn recent_selection_survives_refresh_by_id() {
        let mut list = SessionList::new();
        list.set_recent(vec![recent("b"), recent("a")]);
        list.recent_move_next();
        assert_eq!(list.recent_selected_id().as_deref(), Some("a"));
        // Refresh with a new row ahead: selection stays on the same id.
        list.set_recent(vec![recent("c"), recent("b"), recent("a")]);
        assert_eq!(list.recent_selected_id().as_deref(), Some("a"));
    }

    #[test]
    fn recent_filter_matches_id_and_status() {
        let mut list = SessionList::new();
        list.set_recent(vec![recent("exec-alpha"), recent("exec-beta")]);
        assert_eq!(list.recent_visible().len(), 2);
        list.recent_filter_push('a');
        list.recent_filter_push('l');
        list.recent_filter_push('p');
        list.recent_filter_push('h');
        list.recent_filter_push('a');
        assert_eq!(list.recent_visible().len(), 1);
        assert_eq!(list.recent_visible()[0].id.as_str(), "exec-alpha");
        list.clear_recent_filter();
        assert_eq!(list.recent_visible().len(), 2);
    }

    #[test]
    fn recent_navigation_wraps_over_filtered_rows() {
        let mut list = SessionList::new();
        list.set_recent(vec![recent("b"), recent("a")]);
        list.recent_move_prev();
        assert_eq!(list.recent_selected(), 1);
        list.recent_move_next();
        assert_eq!(list.recent_selected(), 0);
        list.recent_move_next();
        assert_eq!(list.recent_selected(), 1);
    }

    #[test]
    fn relative_age_labels_minutes_hours_days() {
        assert_eq!(relative_age(0, 1_000), "-");
        assert_eq!(relative_age(1_000, 59_000), "now");
        assert_eq!(relative_age(1_000, 61_000), "1m");
        assert_eq!(relative_age(1_000, 301_000), "5m");
        assert_eq!(relative_age(1_000, 7_201_000), "2h");
        assert_eq!(relative_age(1_000, 259_201_000), "3d");
    }
}
