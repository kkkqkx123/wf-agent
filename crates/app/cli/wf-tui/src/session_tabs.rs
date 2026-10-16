//! Multi-session tab management for the full-screen TUI shell.
//!
//! One TUI process keeps one [`SessionTabs`] holding a live
//! [`InteractiveController`] per open chat tab. All tabs share the cloned
//! [`WfClient`] handle, the terminal backend and the render pipeline owned
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
    pub last_active_ms: u64,
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
            last_active_ms: now_ms,
        }
    }

    pub fn streaming(&self) -> bool {
        self.controller.is_processing()
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

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, SessionTab> {
        self.tabs.iter_mut()
    }

    pub fn titles(&self) -> Vec<String> {
        self.tabs.iter().map(|tab| tab.title.clone()).collect()
    }

    /// Per-tab `(title, streaming)` snapshot for the tab strip. The strip
    /// only reads this; it never borrows a controller mutably.
    pub fn snapshot(&self) -> Vec<(String, bool)> {
        self.tabs
            .iter()
            .map(|tab| (tab.title.clone(), tab.streaming()))
            .collect()
    }

    /// Whether any background (non-active) tab is currently streaming.
    pub fn background_streaming(&self) -> bool {
        self.tabs
            .iter()
            .enumerate()
            .any(|(index, tab)| index != self.active && tab.streaming())
    }

    /// Push a tab and focus it.
    pub fn push(&mut self, mut tab: SessionTab) {
        for (index, other) in self.tabs.iter_mut().enumerate() {
            if index != self.active {
                other.holder.detach();
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
            } else {
                tab.holder.detach();
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
        }
        Some(tab)
    }

    /// Index of the eviction victim when the tab list is full: the
    /// least-recently-active non-active tab.
    pub fn eviction_victim(&self) -> Option<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.active)
            .min_by_key(|(_, tab)| tab.last_active_ms)
            .map(|(index, _)| index)
    }

    /// Detach every holder (leaving the Interactive screen). Controllers
    /// and their stream tasks stay alive so returning re-attaches cleanly.
    pub fn detach_all(&mut self) {
        for tab in &mut self.tabs {
            tab.holder.detach();
        }
    }

    /// Re-attach the active holder (returning to the Interactive screen).
    pub fn attach_active(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.holder.reattached();
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

    #[test]
    fn title_shortens_long_execution_ids() {
        assert_eq!(title_for_execution("exec-alpha"), "exec-alp...");
        assert_eq!(title_for_execution("short"), "short");
    }
}
