//! Session list browser state: selection over the open chat tabs plus a
//! recent-sessions pane.
//!
//! The browser is transient and never enters the navigation stack. It owns
//! only the selected row so any screen, including the session screen, can
//! open it without leaving the current page. Rendering reads the live
//! [`crate::session_tabs::SessionTabs`] snapshot directly; recent sessions
//! arrive from a background fetch of the agent-loop summaries.
//!
//! The recent pane is continuable: Enter switches to the open tab when the
//! session is already open, otherwise it opens the session in a new tab and
//! replays its server conversation into the turn history. Typing filters
//! over id, title and status; `d` deletes and `r` renames to the filter.

use tui_core::screen_data::short_id;

/// One recent session row from the agent-loop summaries.
#[derive(Debug, Clone)]
pub struct RecentSession {
    pub id: String,
    pub status: String,
    /// Pre-formatted relative age (`5m`, `2h`, `3d`).
    pub started: String,
    /// Server display title, when the loop has one.
    pub title: Option<String>,
}

impl RecentSession {
    /// Single display line for the recent pane: title (or short id) first so
    /// renamed sessions are recognizable, then status and relative age.
    pub fn display(&self) -> String {
        let label = self
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or_default();
        if label.is_empty() {
            format!(
                "{} · {} · {}",
                short_id(&self.id),
                self.status,
                self.started
            )
        } else {
            format!(
                "{} · {} · {} · {}",
                short_id(&self.id),
                label,
                self.status,
                self.started
            )
        }
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
    named_only: bool,
    pending_delete_id: Option<String>,
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

    /// Switch between the tabs and recent panes. A pending delete
    /// confirmation never survives the switch.
    pub fn toggle_pane(&mut self) {
        self.pending_delete_id = None;
        self.pane = match self.pane {
            SessionListPane::Tabs => SessionListPane::Recent,
            SessionListPane::Recent => SessionListPane::Tabs,
        };
    }

    /// Replace the recent rows (already newest-first from the fetch),
    /// keeping the selection on the same session id when listed.
    /// A refresh drops any pending delete confirmation: the rows the
    /// operator armed may be gone.
    pub fn set_recent(&mut self, rows: Vec<RecentSession>) {
        self.pending_delete_id = None;
        self.recent = rows;
        self.restore_recent_selection();
    }

    pub fn recent_filter(&self) -> &str {
        &self.recent_filter
    }

    pub fn recent_selected(&self) -> usize {
        self.recent_selected
    }

    /// Whether the recent pane has any fetched rows at all.
    pub fn recent_is_empty(&self) -> bool {
        self.recent.is_empty()
    }

    /// Total fetched recent rows, before filtering.
    pub fn recent_total(&self) -> usize {
        self.recent.len()
    }

    /// Selected recent row, when a visible row exists.
    pub fn recent_selected_row(&self) -> Option<RecentSession> {
        self.recent_selected_id()
            .and_then(|id| self.recent.iter().find(|row| row.id == id).cloned())
    }

    /// Whether only titled sessions are shown in the recent pane.
    pub fn named_only(&self) -> bool {
        self.named_only
    }

    /// Toggle the named-only filter, dropping any pending delete.
    pub fn toggle_named_only(&mut self) {
        self.named_only = !self.named_only;
        self.pending_delete_id = None;
        self.restore_recent_selection();
    }

    /// Session id armed for deletion by a first Ctrl+D, if any.
    pub fn pending_delete_id(&self) -> Option<&str> {
        self.pending_delete_id.as_deref()
    }

    /// Arm a deletion: the next Ctrl+D on the same id confirms.
    pub fn arm_delete(&mut self, id: String) {
        self.pending_delete_id = Some(id);
    }

    /// Confirm a deletion when the armed id matches, clearing the arm
    /// either way the caller proceeds.
    pub fn confirm_delete(&mut self, id: &str) -> bool {
        if self.pending_delete_id.as_deref() == Some(id) {
            self.pending_delete_id = None;
            true
        } else {
            false
        }
    }

    /// Cancel an armed deletion. Returns true when one was armed.
    pub fn cancel_pending_delete(&mut self) -> bool {
        if self.pending_delete_id.is_some() {
            self.pending_delete_id = None;
            true
        } else {
            false
        }
    }

    /// Recent rows matching the current filter: whitespace-separated
    /// tokens over id, title and status (`"phrase"` matches exactly),
    /// all tokens required. An empty filter keeps the fetch order
    /// (newest first); otherwise title hits rank before id/status hits
    /// with a stable sort. `named_only` hides untitled sessions.
    pub fn recent_visible(&self) -> Vec<&RecentSession> {
        let scoped: Vec<&RecentSession> = if self.named_only {
            self.recent
                .iter()
                .filter(|row| {
                    row.title
                        .as_deref()
                        .is_some_and(|title| !title.trim().is_empty())
                })
                .collect()
        } else {
            self.recent.iter().collect()
        };
        let tokens = parse_recent_tokens(&self.recent_filter);
        if tokens.is_empty() {
            return scoped;
        }
        let mut scored: Vec<(&RecentSession, u32)> = scoped
            .into_iter()
            .filter_map(|row| recent_row_score(row, &tokens).map(|score| (row, score)))
            .collect();
        scored.sort_by_key(|(_, score)| *score);
        scored.into_iter().map(|(row, _)| row).collect()
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
        self.pending_delete_id = None;
        self.recent_filter.push(c);
        self.restore_recent_selection();
    }

    pub fn recent_filter_backspace(&mut self) {
        self.pending_delete_id = None;
        self.recent_filter.pop();
        self.restore_recent_selection();
    }

    pub fn clear_recent_filter(&mut self) {
        if self.recent_filter.is_empty() {
            return;
        }
        self.pending_delete_id = None;
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

/// One parsed recent filter token: a phrase requires a normalized exact
/// hit, a word a case-insensitive substring hit. All tokens must match.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RecentToken {
    phrase: bool,
    value: String,
}

/// Split a filter into tokens, honoring double quotes for exact phrases.
/// Unbalanced quotes fall back to plain whitespace splitting.
fn parse_recent_tokens(query: &str) -> Vec<RecentToken> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in trimmed.chars() {
        if ch == '"' {
            if in_quote {
                let value = current.trim().to_string();
                if !value.is_empty() {
                    tokens.push(RecentToken {
                        phrase: true,
                        value,
                    });
                }
                current.clear();
                in_quote = false;
            } else {
                let value = current.trim().to_string();
                if !value.is_empty() {
                    tokens.push(RecentToken {
                        phrase: false,
                        value,
                    });
                }
                current.clear();
                in_quote = true;
            }
            continue;
        }
        if !in_quote && ch.is_whitespace() {
            let value = current.trim().to_string();
            if !value.is_empty() {
                tokens.push(RecentToken {
                    phrase: false,
                    value,
                });
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }
    if in_quote {
        return trimmed
            .split_whitespace()
            .map(|part| RecentToken {
                phrase: false,
                value: part.to_string(),
            })
            .collect();
    }
    let value = current.trim().to_string();
    if !value.is_empty() {
        tokens.push(RecentToken {
            phrase: false,
            value,
        });
    }
    tokens
}

/// Lowercase and collapse whitespace so `"deploy  api"` matches titles
/// with different spacing.
fn normalize_recent_text(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Score of one row for one token: title hits rank first, then id, then
/// status. `None` means no hit.
fn recent_token_score(row: &RecentSession, token: &RecentToken) -> Option<u32> {
    if token.phrase {
        let needle = normalize_recent_text(&token.value);
        if needle.is_empty() {
            return Some(0);
        }
        if let Some(title) = row.title.as_deref() {
            if normalize_recent_text(title).contains(needle.as_str()) {
                return Some(0);
            }
        }
        if normalize_recent_text(&row.id).contains(needle.as_str()) {
            return Some(1);
        }
        if normalize_recent_text(&row.status).contains(needle.as_str()) {
            return Some(2);
        }
        return None;
    }
    let needle = token.value.to_lowercase();
    if let Some(title) = row.title.as_deref() {
        if title.to_lowercase().contains(needle.as_str()) {
            return Some(0);
        }
    }
    if row.id.to_lowercase().contains(needle.as_str()) {
        return Some(1);
    }
    if row.status.to_lowercase().contains(needle.as_str()) {
        return Some(2);
    }
    None
}

/// Total score of one row for all tokens, or `None` when any token
/// misses (AND semantics).
fn recent_row_score(row: &RecentSession, tokens: &[RecentToken]) -> Option<u32> {
    let mut total = 0u32;
    for token in tokens {
        total += recent_token_score(row, token)?;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recent(id: &str) -> RecentSession {
        RecentSession {
            id: id.to_string(),
            status: "completed".to_string(),
            started: "5m".to_string(),
            title: None,
        }
    }
    fn titled(id: &str, title: &str) -> RecentSession {
        RecentSession {
            id: id.to_string(),
            status: "completed".to_string(),
            started: "5m".to_string(),
            title: Some(title.to_string()),
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
    fn recent_filter_matches_title() {
        let mut list = SessionList::new();
        list.set_recent(vec![
            titled("exec-alpha", "deploy api"),
            recent("exec-beta"),
        ]);
        list.recent_filter_push('d');
        list.recent_filter_push('e');
        list.recent_filter_push('p');
        list.recent_filter_push('l');
        list.recent_filter_push('o');
        list.recent_filter_push('y');
        assert_eq!(list.recent_visible().len(), 1);
        assert_eq!(list.recent_visible()[0].id.as_str(), "exec-alpha");
    }

    #[test]
    fn recent_display_prefers_title_when_present() {
        let row = titled("exec-alpha-long-id", "deploy api");
        let line = row.display();
        assert!(line.contains("deploy api"), "{line}");
        let plain = recent("exec-beta");
        assert!(!plain.display().contains("·  ·"), "{}", plain.display());
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

    fn push_filter(list: &mut SessionList, text: &str) {
        for ch in text.chars() {
            list.recent_filter_push(ch);
        }
    }

    #[test]
    fn recent_phrase_matches_exact_spacing_insensitive() {
        let mut list = SessionList::new();
        list.set_recent(vec![
            titled("a", "deploy   API hotfix"),
            titled("b", "api deploy reversed"),
        ]);
        push_filter(&mut list, "\"deploy api\"");
        let visible = list.recent_visible();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id.as_str(), "a");
    }

    #[test]
    fn recent_tokens_require_all_terms() {
        let mut list = SessionList::new();
        list.set_recent(vec![
            titled("a", "deploy api"),
            titled("b", "deploy worker"),
        ]);
        push_filter(&mut list, "deploy api");
        let visible = list.recent_visible();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id.as_str(), "a");
    }

    #[test]
    fn recent_ranks_title_before_status() {
        let mut list = SessionList::new();
        list.set_recent(vec![titled("a", "other work"), titled("b", "deploy api")]);
        // Make the first row match on status instead of title.
        list.set_recent(vec![
            RecentSession {
                id: "a".to_string(),
                status: "deploying".to_string(),
                started: "5m".to_string(),
                title: Some("other work".to_string()),
            },
            titled("b", "deploy api"),
        ]);
        push_filter(&mut list, "deploy");
        let visible = list.recent_visible();
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].id.as_str(), "b");
    }

    #[test]
    fn recent_named_only_hides_untitled() {
        let mut list = SessionList::new();
        list.set_recent(vec![titled("a", "deploy"), recent("b")]);
        assert_eq!(list.recent_visible().len(), 2);
        list.toggle_named_only();
        assert!(list.named_only());
        let visible = list.recent_visible();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id.as_str(), "a");
        list.toggle_named_only();
        assert_eq!(list.recent_visible().len(), 2);
    }

    #[test]
    fn recent_delete_needs_confirm_and_esc_cancels() {
        let mut list = SessionList::new();
        list.set_recent(vec![recent("a"), recent("b")]);
        // First arm does not delete: confirm on another id fails.
        list.arm_delete("a".to_string());
        assert_eq!(list.pending_delete_id(), Some("a"));
        assert!(!list.confirm_delete("b"));
        assert!(list.confirm_delete("a"));
        assert_eq!(list.pending_delete_id(), None);
        // Arm again, then cancel.
        list.arm_delete("b".to_string());
        assert!(list.cancel_pending_delete());
        assert!(!list.cancel_pending_delete());
    }

    #[test]
    fn recent_filter_change_cancels_armed_delete() {
        let mut list = SessionList::new();
        list.set_recent(vec![recent("a")]);
        list.arm_delete("a".to_string());
        list.recent_filter_push('x');
        assert_eq!(list.pending_delete_id(), None);
    }
}
