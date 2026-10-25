//! Layered state management for the TUI application.
//!
//! [`AppState`] is the single application state owning three duties:
//! navigation data ([`ScreenState`]), session overlay
//! ([`crate::session_holder::SessionHolder`] lives beside it in the shell)
//! and interface notices ([`NoticeQueue`] plus client focus). The shell keeps
//! one [`AppState`] instead of scattering flat fields, so cache freshness,
//! notice expiry and focus gating stay testable in isolation.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::screens::{
    apply_list_filter, catalog_tab_index, filter_catalog, CatalogKind, ExecStatusFilter, ExecView,
    ScreenData, ScreenKind, Screens, WorkflowView,
};

/// Unified application state: navigation data, interface notices and client
/// focus in one place. Session content itself stays in
/// [`crate::interactive::InteractiveController`] (scroll preparation plus
/// stream pump); this state only tracks where the user is and what transient
/// chrome is visible.
#[derive(Debug)]
pub struct AppState {
    /// Navigation, data cache, drafts and filter.
    pub screen: ScreenState,
    /// Transient status/error line rendered under the screen.
    pub notice: NoticeQueue,
    /// Whether the client terminal currently holds focus. Starts focused;
    /// any key / mouse / paste delivery re-asserts it.
    pub focused: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            screen: ScreenState::new(),
            notice: NoticeQueue::new(),
            focused: true,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Key identifying one cached detail pane: the owning screen, the selected
/// row id and the subview index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailKey {
    pub kind: ScreenKind,
    pub id: String,
    pub view: usize,
}

/// Cached detail lines backing the executions/workflow subview panes.
#[derive(Debug, Clone)]
pub struct DetailCache {
    pub key: DetailKey,
    pub title: String,
    pub lines: Vec<String>,
    pub at: Instant,
}

/// Screen navigation state.
#[derive(Debug)]
pub struct ScreenState {
    pub navigation: Screens,
    pub data_cache: HashMap<ScreenKind, (ScreenData, Instant)>,
    pub inflight: HashMap<ScreenKind, Instant>,
    pub exec_filter: ExecStatusFilter,
    pub settings_scroll: usize,
    /// Active subview on the executions screen.
    pub exec_view: ExecView,
    /// Active subview on the workflow screen.
    pub workflow_view: WorkflowView,
    /// Active catalog tab: `None` is the merged All view.
    pub catalog_tab: Option<CatalogKind>,
    /// Per-tab selection memory, slot 0 is All and slots 1-4 follow
    /// [`CatalogKind::ALL`] order.
    pub catalog_positions: [usize; 5],
    /// Client-side substring filter applied to list screens; cleared on
    /// navigation.
    pub list_filter: String,
    /// Scroll offset inside the detail pane.
    pub detail_scroll: usize,
    /// Cached detail lines for the current selection.
    pub detail: Option<DetailCache>,
    /// In-flight detail fetch marker.
    pub detail_inflight: Option<(DetailKey, Instant)>,
    /// Server truncation flags per list screen.
    pub truncated: HashMap<ScreenKind, bool>,
    /// Workspace file whose version timeline the timeline screen follows;
    /// captured when the screen is opened from the Files screen.
    pub file_timeline_path: Option<String>,
}

impl ScreenState {
    pub fn new() -> Self {
        Self {
            navigation: Screens::new(),
            data_cache: HashMap::new(),
            inflight: HashMap::new(),
            exec_filter: ExecStatusFilter::All,
            settings_scroll: 0,
            exec_view: ExecView::Overview,
            workflow_view: WorkflowView::Summary,
            catalog_tab: None,
            catalog_positions: [0; 5],
            list_filter: String::new(),
            detail_scroll: 0,
            detail: None,
            detail_inflight: None,
            truncated: HashMap::new(),
            file_timeline_path: None,
        }
    }

    /// Current screen kind.
    pub fn current_kind(&self) -> ScreenKind {
        self.navigation.current_kind()
    }

    /// Selected index on the current screen.
    pub fn selected(&self) -> usize {
        self.navigation.selected()
    }

    /// Whether fresh data is cached for `kind`.
    pub fn is_fresh(&self, kind: ScreenKind, ttl: Duration) -> bool {
        self.data_cache
            .get(&kind)
            .map(|(_, at)| at.elapsed() < ttl)
            .unwrap_or(false)
    }

    /// Whether a fetch is in flight for `kind`.
    pub fn has_inflight(&self, kind: ScreenKind, timeout: Duration) -> bool {
        self.inflight
            .get(&kind)
            .map(|started| started.elapsed() < timeout)
            .unwrap_or(false)
    }

    /// Get cached data for the current screen.
    pub fn current_data(&self) -> ScreenData {
        self.data_cache
            .get(&self.current_kind())
            .map(|(data, _)| data.clone())
            .unwrap_or(ScreenData::None)
    }

    /// Cached data with the client-side filter applied. Selection,
    /// navigation and detail fetching all use this so indices stay aligned
    /// with what the screen draws. The catalog additionally applies its
    /// active tab here so the same index drives keys and rendering.
    pub fn display_data(&self) -> ScreenData {
        if self.current_kind() == ScreenKind::Catalog {
            let rows = match self.current_data() {
                ScreenData::Catalog(rows) => rows,
                other => return other,
            };
            return ScreenData::Catalog(filter_catalog(&rows, self.catalog_tab, &self.list_filter));
        }
        let data = self.current_data();
        if self.list_filter.trim().is_empty() {
            data
        } else {
            apply_list_filter(&data, &self.list_filter)
        }
    }

    /// Whether the cached list for `kind` was server-truncated.
    pub fn is_truncated(&self, kind: ScreenKind) -> bool {
        self.truncated.get(&kind).copied().unwrap_or(false)
    }

    /// Switch the catalog tab, remembering the old tab position and
    /// restoring the new one. The filter stays untouched so a query in
    /// All carries over to the single-kind view.
    pub fn switch_catalog_tab(&mut self, next: Option<CatalogKind>) {
        if self.catalog_tab == next {
            return;
        }
        let old = catalog_tab_index(self.catalog_tab);
        if old < self.catalog_positions.len() {
            self.catalog_positions[old] = self.navigation.selected();
        }
        self.catalog_tab = next;
        self.detail_scroll = 0;
        let slot = catalog_tab_index(next);
        let restored = self.catalog_positions.get(slot).copied().unwrap_or(0);
        self.navigation.set_selected(restored);
    }

    /// Reset the catalog to the All tab with cleared per-tab positions.
    pub fn reset_catalog(&mut self) {
        self.catalog_tab = None;
        self.catalog_positions = [0; 5];
    }

    /// Cached detail lines when they belong to `key`.
    pub fn detail_for(&self, key: &DetailKey) -> Option<&DetailCache> {
        self.detail.as_ref().filter(|cached| cached.key == *key)
    }

    /// Drop cached data for `kind`.
    pub fn invalidate(&mut self, kind: ScreenKind) {
        self.data_cache.remove(&kind);
        self.truncated.remove(&kind);
    }
}

impl Default for ScreenState {
    fn default() -> Self {
        Self::new()
    }
}

/// Focus target within the current screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Scrollback,
    Composer,
    Modal,
    Overlay,
}

/// A queue of transient notice messages with TTL, driven by the injectable
/// clock so expiry is deterministic in tests.
#[derive(Debug)]
pub struct NoticeQueue {
    current: Option<(String, u64)>,
    ttl_ms: u64,
}

impl NoticeQueue {
    pub fn new() -> Self {
        Self {
            current: None,
            ttl_ms: Duration::from_secs(6).as_millis() as u64,
        }
    }

    /// Set a notice (replaces any active notice).
    pub fn set(&mut self, text: impl Into<String>) {
        self.current = Some((text.into(), crate::clock::now_ms()));
    }

    /// Drop the active notice.
    pub fn clear(&mut self) {
        self.current = None;
    }

    /// Get the current notice text if not expired.
    pub fn current_text(&self) -> Option<String> {
        let (text, at) = self.current.as_ref()?;
        if crate::clock::now_ms().saturating_sub(*at) < self.ttl_ms {
            Some(text.clone())
        } else {
            None
        }
    }

    /// Expire the notice if its TTL has elapsed. Returns whether a repaint
    /// is needed.
    pub fn expire(&mut self) -> bool {
        if let Some((_, at)) = &self.current {
            if crate::clock::now_ms().saturating_sub(*at) >= self.ttl_ms {
                self.current = None;
                return true;
            }
        }
        false
    }

    /// Whether a notice is currently active (forcing repaints).
    pub fn is_active(&self) -> bool {
        self.current_text().is_some()
    }
}

impl Default for NoticeQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_state_fresh_detection() {
        let mut state = ScreenState::new();
        let ttl = Duration::from_secs(5);
        assert!(!state.is_fresh(ScreenKind::Dashboard, ttl));
        state
            .data_cache
            .insert(ScreenKind::Dashboard, (ScreenData::None, Instant::now()));
        assert!(state.is_fresh(ScreenKind::Dashboard, ttl));
    }

    #[test]
    fn notice_queue_expiry() {
        let mut queue = NoticeQueue::new();
        queue.set("hello");
        assert!(queue.current_text().is_some());
        assert!(!queue.expire());
    }
}
