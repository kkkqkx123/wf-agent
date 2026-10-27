//! Session tab lifecycle for the TUI shell: creation, eviction, pinning,
//! persistence and restore, plus the replay / exit handoffs that drive the
//! tab set from the event loop.

use crate::interactive::InteractiveController;
use crate::overlay::OverlayMode;
use crate::screens::ScreenKind;
use crate::session_tabs::SessionTab;

use super::input_classify::{load_session_scrollbar_opt_in, load_tool_output_config};
use super::TuiApp;

impl TuiApp {
    /// Create the first tab or park all tabs to match the current screen.
    /// Tabs survive navigation: leaving the Interactive screen only detaches
    /// the per-tab holders, and returning re-attaches the active one, so no
    /// scrollback or stream state is lost. The first open restores pinned
    /// tabs from the persisted snapshot before creating a fresh tab.
    pub(super) async fn ensure_interactive(&mut self) -> crate::error::CliResult<()> {
        let on_interactive = self.app.screen.navigation.current_kind() == ScreenKind::Interactive;
        if on_interactive {
            if self.sessions.is_empty() {
                self.restore_pinned_tabs().await;
                if self.sessions.is_empty() {
                    self.new_tab().await;
                }
            } else {
                self.sessions.attach_active();
            }
        } else {
            self.sessions.detach_all();
        }
        Ok(())
    }

    /// Create a new chat session tab over the shared client and focus it.
    /// When the tab list is full the least-recently-active unpinned
    /// background tab is evicted first.
    pub(super) async fn new_tab(&mut self) {
        self.new_tab_with_parent(None).await;
    }

    /// Create a new tab optionally linked to a parent session id. Child
    /// tabs record the linkage so parent/child navigation can find them.
    pub(super) async fn new_tab_with_parent(&mut self, parent_id: Option<String>) {
        if self.sessions.len() >= tui_core::screen_data::MAX_CHAT_TABS {
            if let Some(victim) = self.sessions.eviction_victim() {
                if let Some(tab) = self.sessions.remove(victim) {
                    tab.controller.shutdown().await;
                }
                self.app
                    .notice
                    .set("tab limit reached: oldest unpinned tab closed".to_string());
            }
        }
        let id = wf_common::generate_id();
        let mut session = InteractiveController::start(self.client.clone(), id.clone()).await;
        session.set_keymap(self.keymap.clone());
        session.set_perf_tier(self.perf_tier.marker());
        session.apply_perf_policy(self.perf_policy);
        session.apply_copy_policy(self.copy_policy);
        session.set_tool_config(load_tool_output_config());
        session.set_session_scrollbar(load_session_scrollbar_opt_in());
        let workspace = std::env::current_dir()
            .ok()
            .and_then(|dir| dir.to_str().map(str::to_string));
        let mut tab = SessionTab::new(session, id, self.now_ms());
        tab.parent_id = parent_id;
        tab.workspace = workspace;
        self.sessions.push(tab);
        self.save_tabs();
    }

    /// Close the tab at `index`, shutting its stream tasks down.
    pub(super) async fn close_tab(&mut self, index: usize) {
        if let Some(tab) = self.sessions.remove(index) {
            tab.controller.shutdown().await;
        }
        self.save_tabs();
        if self.sessions.is_empty() {
            self.go_back();
        }
    }

    /// Toggle pin on the active tab. Pinned tabs skip LRU eviction and are
    /// persisted across restarts.
    pub(super) fn toggle_pin_active(&mut self) {
        let index = self.sessions.active_index();
        if let Some(pinned) = self.sessions.toggle_pin(index) {
            self.app.notice.set(if pinned {
                format!("tab {} pinned", index + 1)
            } else {
                format!("tab {} unpinned", index + 1)
            });
            self.save_tabs();
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
        }
    }

    /// Move the active tab one slot toward the start or end.
    pub(super) fn move_active(&mut self, delta: i32) {
        let from = self.sessions.active_index();
        let len = self.sessions.len();
        if len < 2 {
            return;
        }
        let to = if delta >= 0 {
            (from + 1) % len
        } else {
            (from + len - 1) % len
        };
        if self.sessions.move_tab(from, to) {
            self.app.notice.set(format!("tab moved to {}", to + 1));
            self.save_tabs();
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
        }
    }

    /// Open the session list browser over the current tab set.
    pub(super) fn open_session_list(&mut self) {
        self.session_list
            .open(self.sessions.active_index(), self.sessions.len());
        self.overlay = OverlayMode::SessionList;
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Snapshot the open tabs for persistence.
    fn tabs_snapshot(&self) -> crate::tab_persistence::SavedTabs {
        crate::tab_persistence::SavedTabs {
            version: crate::tab_persistence::TABS_SNAPSHOT_VERSION,
            active: self.sessions.active_index(),
            tabs: self
                .sessions
                .iter()
                .map(|tab| crate::tab_persistence::SavedTab {
                    session_id: tab
                        .execution_id()
                        .or_else(|| tab.holder.session_id())
                        .unwrap_or_default()
                        .to_string(),
                    title: tab.title.clone(),
                    title_is_default: tab.title_is_default,
                    pinned: tab.pinned,
                    parent_id: tab.parent_id.clone(),
                    model: tab.model.clone(),
                    agent: tab.agent.clone(),
                    workspace: tab.workspace.clone(),
                    last_active_ms: tab.last_active_ms,
                })
                .collect(),
        }
    }

    pub(super) fn save_tabs(&self) {
        crate::tab_persistence::save(&self.tabs_snapshot());
    }

    /// Live execution ids known from the cached executions screen. Used
    /// to prune tabs whose server sessions vanished. Returns `None` when
    /// no executions snapshot is cached yet so callers prune nothing.
    pub(super) fn live_execution_ids(&self) -> Option<std::collections::HashSet<String>> {
        let (data, _) = self.app.screen.data_cache.get(&ScreenKind::Executions)?;
        let crate::screens::ScreenData::Executions(rows) = data else {
            return None;
        };
        Some(rows.iter().map(|row| row.id.clone()).collect())
    }

    /// Restore pinned tabs from the persisted snapshot. Called once when
    /// the first interactive screen opens with an empty tab list.
    async fn restore_pinned_tabs(&mut self) {
        let Some(saved) = crate::tab_persistence::load() else {
            return;
        };
        if saved.tabs.is_empty() {
            return;
        }
        for entry in saved
            .tabs
            .into_iter()
            .take(tui_core::screen_data::MAX_CHAT_TABS)
        {
            if entry.session_id.is_empty() {
                continue;
            }
            if self
                .sessions
                .index_of_execution(&entry.session_id)
                .is_some()
            {
                continue;
            }
            let mut session =
                InteractiveController::start(self.client.clone(), entry.session_id.clone()).await;
            session.set_keymap(self.keymap.clone());
            session.set_perf_tier(self.perf_tier.marker());
            session.apply_perf_policy(self.perf_policy);
            session.apply_copy_policy(self.copy_policy);
            session.set_tool_config(load_tool_output_config());
            session.set_session_scrollbar(load_session_scrollbar_opt_in());
            session.load_replay(&entry.session_id);
            let mut tab = SessionTab::new(session, entry.session_id.clone(), entry.last_active_ms);
            tab.title = entry.title;
            tab.title_is_default = entry.title_is_default;
            tab.pinned = entry.pinned;
            tab.parent_id = entry.parent_id;
            tab.model = entry.model;
            tab.agent = entry.agent;
            tab.workspace = entry.workspace;
            self.sessions.push(tab);
        }
        let active = saved.active.min(self.sessions.len().saturating_sub(1));
        self.sessions.switch(active, self.now_ms());
    }

    /// Drop tabs whose sessions vanished from the server list. Only
    /// idle unpinned background tabs are eligible: the active tab,
    /// streaming tabs, pinned tabs and tabs touched in the last minute
    /// (fresh local tabs without a server record yet) are always kept.
    /// Returns the number of pruned tabs for notice reporting.
    pub(super) async fn prune_closed_tabs(
        &mut self,
        live: &std::collections::HashSet<String>,
    ) -> usize {
        if self.sessions.is_empty() {
            return 0;
        }
        let now = self.now_ms();
        let active = self.sessions.active_index();
        let mut victims = Vec::new();
        for (index, tab) in self.sessions.iter().enumerate() {
            if index == active || tab.pinned || tab.streaming() {
                continue;
            }
            let id = tab
                .execution_id()
                .or_else(|| tab.holder.session_id())
                .unwrap_or_default();
            if live.contains(id) {
                continue;
            }
            if now.saturating_sub(tab.last_active_ms) < 60_000 {
                continue;
            }
            victims.push(index);
        }
        let mut count = 0;
        for index in victims.into_iter().rev() {
            if self.sessions.get(index).is_some() {
                if let Some(tab) = self.sessions.remove(index) {
                    tab.controller.shutdown().await;
                    count += 1;
                }
            }
        }
        if count > 0 {
            self.save_tabs();
        }
        count
    }

    /// Queue a session restore from `--session` / `--resume`: the replay
    /// loads once the interactive controller attaches.
    pub async fn queue_session_restore(&mut self, session: Option<String>, resume: bool) {
        if let Some(id) = session {
            self.pending_replay = Some(id);
        } else if resume {
            match crate::replay::latest_session_id(&self.client).await {
                Ok(Some(id)) => self.pending_replay = Some(id),
                Ok(None) => self.app.notice.set("no previous session found".to_string()),
                Err(err) => self.app.notice.set(format!("resume failed: {err}")),
            }
        }
    }

    /// Leave the interactive screen when the active tab requests it: close
    /// the active tab and go back. Other tabs keep streaming.
    pub(super) async fn apply_interactive_exit(&mut self) {
        if !self.interactive_exit {
            return;
        }
        self.interactive_exit = false;
        if let Some(session) = self.sessions.active_mut() {
            session.begin_graceful_exit();
        }
        let active = self.sessions.active_index();
        self.close_tab(active).await;
    }

    /// Load replay history into the active tab after navigating from
    /// Executions. The replay replaces the active tab's scrollback exactly
    /// like the previous single-session behavior; open a new tab first
    /// when the current session must be kept.
    pub(super) async fn apply_pending_replay(&mut self) -> crate::error::CliResult<()> {
        let Some(id) = self.pending_replay.take() else {
            return Ok(());
        };
        self.ensure_interactive().await?;
        if let Some(session) = self.sessions.active_mut() {
            // `load_replay` fetches asynchronously; the `ReplayLoaded` event
            // replaces the loading placeholder when the history lands.
            session.load_replay(&id);
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
        }
        Ok(())
    }
}
