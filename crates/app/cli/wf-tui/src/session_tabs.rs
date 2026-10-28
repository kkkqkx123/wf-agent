//! Multi-session tab management for the full-screen TUI shell.
//!
//! One TUI process keeps one [`SessionTabs`] holding a live
//! [`InteractiveController`] per open chat tab. All tabs share the cloned
//! [`wf_client::WfClient`] handle, the terminal backend and the render pipeline owned
//! by the shell; each tab keeps its own scrollback, reducer, footer and
//! stream tasks. Only the active tab receives input and paints; background
//! tabs keep pumping their event channels so their streams continue.
//!
//! Each tab owns a [`SessionHolder`]: the single holder that used to live
//! on the shell becomes one holder per tab.

use crate::interactive::InteractiveController;
use crate::screens::short_id;
use crate::session_holder::SessionHolder;

/// One open chat session tab.
pub struct SessionTab {
    pub controller: InteractiveController,
    pub holder: SessionHolder,
    pub title: String,
    /// Whether the title is still the generated default and may be replaced
    /// by content-based auto-naming.
    pub title_is_default: bool,
    pub last_active_ms: u64,
    pub pinned: bool,
    pub parent_id: Option<String>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub workspace: Option<String>,
}

impl SessionTab {
    pub fn new(
        controller: InteractiveController,
        execution_id: impl Into<String>,
        now_ms: u64,
    ) -> Self {
        let execution_id = execution_id.into();
        let mut holder = SessionHolder::new();
        holder.hold(execution_id.clone());
        holder.attach();
        let title = title_for_execution(&execution_id);
        Self {
            controller,
            holder,
            title,
            title_is_default: true,
            last_active_ms: now_ms,
            pinned: false,
            parent_id: None,
            model: None,
            agent: None,
            workspace: None,
        }
    }

    pub fn with_parent(mut self, parent_id: Option<String>) -> Self {
        self.parent_id = parent_id;
        self
    }

    pub fn streaming(&self) -> bool {
        self.controller.is_processing()
    }

    pub fn execution_id(&self) -> Option<&str> {
        self.controller
            .session_id()
            .or_else(|| self.holder.session_id())
    }
}

/// Short display title derived from an execution id.
pub fn title_for_execution(execution_id: &str) -> String {
    short_id(execution_id)
}

/// Live chat session tabs with an active index. The active index stays
/// valid whenever the list is non-empty.
pub struct SessionTabs {
    tabs: Vec<SessionTab>,
    active: usize,
}

impl SessionTabs {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> Option<&InteractiveController> {
        self.tabs.get(self.active).map(|tab| &tab.controller)
    }

    pub fn active_mut(&mut self) -> Option<&mut InteractiveController> {
        self.tabs
            .get_mut(self.active)
            .map(|tab| &mut tab.controller)
    }

    pub fn active_tab(&self) -> Option<&SessionTab> {
        self.tabs.get(self.active)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut SessionTab> {
        self.tabs.get_mut(self.active)
    }

    pub fn get(&self, index: usize) -> Option<&SessionTab> {
        self.tabs.get(index)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, SessionTab> {
        self.tabs.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, SessionTab> {
        self.tabs.iter_mut()
    }

    pub fn titles(&self) -> Vec<String> {
        self.tabs.iter().map(|tab| tab.title.clone()).collect()
    }

    /// Per-tab `(title, streaming, pinned)` snapshot for the tab strip.
    /// The strip only reads this; it never borrows a controller mutably.
    pub fn snapshot(&self) -> Vec<(String, bool, bool)> {
        self.tabs
            .iter()
            .map(|tab| (tab.title.clone(), tab.streaming(), tab.pinned))
            .collect()
    }

    /// Execution ids of all open tabs, oldest first.
    pub fn execution_ids(&self) -> Vec<String> {
        self.tabs
            .iter()
            .filter_map(|tab| tab.execution_id().map(str::to_string))
            .collect()
    }

    /// Whether any background (non-active) tab is currently streaming.
    pub fn background_streaming(&self) -> bool {
        self.tabs
            .iter()
            .enumerate()
            .any(|(index, tab)| index != self.active && tab.streaming())
    }

    /// Toggle the pinned flag of the tab at `index`.
    pub fn toggle_pin(&mut self, index: usize) -> Option<bool> {
        let tab = self.tabs.get_mut(index)?;
        tab.pinned = !tab.pinned;
        Some(tab.pinned)
    }

    pub fn set_active_meta(
        &mut self,
        model: Option<String>,
        agent: Option<String>,
        workspace: Option<String>,
    ) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return false;
        };
        if model.is_some() {
            tab.model = model;
        }
        if agent.is_some() {
            tab.agent = agent;
        }
        if workspace.is_some() {
            tab.workspace = workspace;
        }
        true
    }

    /// Index of the tab holding `execution_id`, if open.
    pub fn index_of_execution(&self, execution_id: &str) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            tab.execution_id().is_some_and(|id| id == execution_id)
                || tab.holder.session_id().is_some_and(|id| id == execution_id)
        })
    }

    /// Indices of direct children of `execution_id`.
    pub fn children_of(&self, execution_id: &str) -> Vec<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.parent_id.as_deref() == Some(execution_id))
            .map(|(index, _)| index)
            .collect()
    }

    /// Focus the parent of the active tab.
    pub fn focus_parent(&mut self, now_ms: u64) -> bool {
        let parent = self
            .active_tab()
            .and_then(|tab| tab.parent_id.clone())
            .and_then(|id| self.index_of_execution(&id));
        match parent {
            Some(index) => self.switch(index, now_ms),
            None => false,
        }
    }

    /// Focus the first child of the active tab.
    pub fn focus_child_first(&mut self, now_ms: u64) -> bool {
        let id = match self.active_tab() {
            Some(tab) => tab.execution_id().map(str::to_string),
            None => None,
        };
        let Some(id) = id else {
            return false;
        };
        match self.children_of(&id).into_iter().next() {
            Some(index) => self.switch(index, now_ms),
            None => false,
        }
    }

    pub fn focus_child_next(&mut self, now_ms: u64) -> bool {
        self.focus_sibling_or_child(1, now_ms)
    }

    pub fn focus_child_prev(&mut self, now_ms: u64) -> bool {
        self.focus_sibling_or_child(-1, now_ms)
    }

    fn focus_sibling_or_child(&mut self, delta: i32, now_ms: u64) -> bool {
        let active_id = match self.active_tab().and_then(|tab| tab.execution_id()) {
            Some(id) => id.to_string(),
            None => return false,
        };
        let scope_id = self
            .active_tab()
            .and_then(|tab| tab.parent_id.clone())
            .unwrap_or_else(|| active_id.clone());
        let mut peers = self.children_of(&scope_id);
        if peers.is_empty() {
            return false;
        }
        peers.sort_unstable();
        let pos = peers
            .iter()
            .position(|index| {
                self.tabs.get(*index).and_then(|tab| tab.execution_id()) == Some(active_id.as_str())
            })
            .unwrap_or(0);
        let next = if delta >= 0 {
            peers[(pos + 1) % peers.len()]
        } else {
            peers[(pos + peers.len() - 1) % peers.len()]
        };
        self.switch(next, now_ms)
    }

    /// Move the tab at `from` to position `to`, keeping the active tab
    /// attached to its session.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }
        let active_id = self
            .active_tab()
            .and_then(|tab| tab.execution_id().map(str::to_string));
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        if let Some(id) = active_id {
            if let Some(index) = self.index_of_execution(&id) {
                self.active = index;
                self.touch_active();
                for (pos, tab) in self.tabs.iter_mut().enumerate() {
                    if pos == self.active {
                        tab.holder.reattached();
                        tab.controller.note_reattached();
                    } else {
                        tab.holder.detach();
                        tab.controller.note_detached();
                    }
                }
            }
        }
        true
    }

    /// Remove tabs whose execution id is absent from `live`. Returns the
    /// removed tabs so the caller can shut their controllers down.
    pub fn prune_missing(&mut self, live: &std::collections::HashSet<String>) -> Vec<SessionTab> {
        let mut removed = Vec::new();
        let mut index = 0;
        while index < self.tabs.len() {
            let alive = self.tabs[index]
                .execution_id()
                .is_some_and(|id| live.contains(id))
                || self.tabs[index]
                    .holder
                    .session_id()
                    .is_some_and(|id| live.contains(id));
            if alive {
                index += 1;
            } else {
                removed.push(self.tabs.remove(index));
                if self.tabs.is_empty() {
                    self.active = 0;
                    break;
                }
                if index < self.active {
                    self.active -= 1;
                } else if self.active >= self.tabs.len() {
                    self.active = self.tabs.len() - 1;
                }
            }
        }
        if let Some(active) = self.tabs.get_mut(self.active) {
            active.holder.reattached();
            active.controller.note_reattached();
        }
        removed
    }

    /// Push a tab and focus it.
    pub fn push(&mut self, mut tab: SessionTab) {
        for (index, other) in self.tabs.iter_mut().enumerate() {
            if index != self.active {
                other.holder.detach();
                other.controller.note_detached();
            }
        }
        tab.holder.attach();
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.touch_active();
    }

    /// Switch the active tab. Returns false on an out-of-range index.
    pub fn switch(&mut self, index: usize, now_ms: u64) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.active = index;
        self.touch_active_at(now_ms);
        for (pos, tab) in self.tabs.iter_mut().enumerate() {
            if pos == self.active {
                tab.holder.reattached();
                tab.controller.note_reattached();
            } else {
                tab.holder.detach();
                tab.controller.note_detached();
            }
        }
        true
    }

    pub fn next(&mut self, now_ms: u64) {
        if self.tabs.is_empty() {
            return;
        }
        let index = (self.active + 1) % self.tabs.len();
        self.switch(index, now_ms);
    }

    pub fn prev(&mut self, now_ms: u64) {
        if self.tabs.is_empty() {
            return;
        }
        let index = (self.active + self.tabs.len().saturating_sub(1)) % self.tabs.len();
        self.switch(index, now_ms);
    }

    /// Remove the tab at `index` and return it; the caller shuts the
    /// controller down. Returns `None` on an out-of-range index.
    pub fn remove(&mut self, index: usize) -> Option<SessionTab> {
        if index >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if index < self.active {
            self.active -= 1;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        self.touch_active();
        if let Some(active) = self.tabs.get_mut(self.active) {
            active.holder.reattached();
            active.controller.note_reattached();
        }
        Some(tab)
    }

    /// Index of the eviction victim when the tab list is full: the
    /// least-recently-active unpinned non-active tab. Pinned tabs are
    /// skipped unless every background tab is pinned.
    pub fn eviction_victim(&self) -> Option<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.active)
            .filter(|(_, tab)| !tab.pinned)
            .min_by_key(|(_, tab)| tab.last_active_ms)
            .map(|(index, _)| index)
            .or_else(|| {
                self.tabs
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != self.active)
                    .min_by_key(|(_, tab)| tab.last_active_ms)
                    .map(|(index, _)| index)
            })
    }

    /// Detach every holder (leaving the Interactive screen). Controllers
    /// and their stream tasks stay alive so returning re-attaches cleanly.
    pub fn detach_all(&mut self) {
        for tab in &mut self.tabs {
            tab.holder.detach();
            tab.controller.note_detached();
        }
    }

    /// Re-attach the active holder (returning to the Interactive screen).
    pub fn attach_active(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.holder.reattached();
            tab.controller.note_reattached();
        }
    }

    fn touch_active(&mut self) {
        let now_ms = crate::clock::now_ms();
        self.touch_active_at(now_ms);
    }

    fn touch_active_at(&mut self, now_ms: u64) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.last_active_ms = now_ms;
        }
    }
}

impl Default for SessionTabs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_client::{ClientConfig, WfClient};

    fn test_client() -> WfClient {
        WfClient::new(ClientConfig::new("http://127.0.0.1:1"))
    }

    async fn test_tab(id: &str, now_ms: u64) -> SessionTab {
        let controller = InteractiveController::start(test_client(), id.to_string()).await;
        SessionTab::new(controller, id, now_ms)
    }

    #[tokio::test]
    async fn push_switch_and_remove_track_active() {
        let mut tabs = SessionTabs::new();
        assert!(tabs.is_empty());
        tabs.next(1);
        tabs.prev(1);
        assert!(!tabs.switch(0, 1));
        assert!(tabs.remove(0).is_none());

        tabs.push(test_tab("exec-alpha", 10).await);
        tabs.push(test_tab("exec-beta", 20).await);
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.active_index(), 1);
        assert!(tabs.switch(0, 30));
        assert_eq!(tabs.active_index(), 0);
        assert_eq!(
            tabs.active().and_then(|tab| tab.session_id()),
            Some("exec-alpha")
        );

        tabs.next(40);
        assert_eq!(tabs.active_index(), 1);
        tabs.prev(50);
        assert_eq!(tabs.active_index(), 0);

        let removed = tabs.remove(0).expect("tab index is valid");
        assert_eq!(removed.title, title_for_execution("exec-alpha"));
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs.active_index(), 0);
        removed.controller.shutdown().await;
    }

    #[tokio::test]
    async fn eviction_picks_least_recently_active_background_tab() {
        use tui_core::screen_data::MAX_CHAT_TABS;

        let mut tabs = SessionTabs::new();
        for index in 0..MAX_CHAT_TABS {
            tabs.push(test_tab(&format!("exec-{index:02}"), 100 + index as u64).await);
        }
        assert_eq!(tabs.len(), MAX_CHAT_TABS);
        tabs.switch(MAX_CHAT_TABS - 1, 1000);
        let victim = tabs.eviction_victim().expect("full tab list has a victim");
        assert_ne!(victim, tabs.active_index());
        assert_eq!(victim, 0);
        let removed = tabs.remove(victim).expect("victim index is valid");
        assert_eq!(removed.title, title_for_execution("exec-00"));
        removed.controller.shutdown().await;
    }

    #[tokio::test]
    async fn pin_move_and_parent_navigation() {
        let mut tabs = SessionTabs::new();
        tabs.push(test_tab("exec-a", 10).await);
        tabs.push(test_tab("exec-b", 20).await);
        assert_eq!(tabs.toggle_pin(0), Some(true));
        assert_eq!(tabs.toggle_pin(0), Some(false));
        assert!(tabs.move_tab(0, 1));
        assert_eq!(tabs.index_of_execution("exec-a"), Some(1));
        tabs.switch(1, 30);
        if let Some(tab) = tabs.active_tab_mut() {
            tab.parent_id = Some("exec-b".to_string());
        }
        assert!(tabs.focus_parent(40));
        assert_eq!(tabs.active_index(), 0);
        assert!(tabs.focus_child_first(50));
        assert_eq!(tabs.active_index(), 1);
    }

    #[test]
    fn title_shortens_long_execution_ids() {
        assert_eq!(title_for_execution("exec-alpha"), "exec-alp...");
        assert_eq!(title_for_execution("short"), "short");
    }
}
