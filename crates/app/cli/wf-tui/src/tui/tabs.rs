//! Session tab lifecycle for the TUI shell: creation, eviction, pinning,
//! persistence and restore, plus the replay / exit handoffs that drive the
//! tab set from the event loop.

use crate::interactive::InteractiveController;
use crate::overlay::OverlayMode;
use crate::screens::ScreenKind;
use crate::session_tabs::SessionTab;

use super::data_cache::DataResult;
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
        session.apply_clear_selection_after_copy(self.clear_selection_after_copy());
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

    /// Open the session list browser over the current tab set and refresh
    /// the recent-sessions pane on a background task.
    pub(super) fn open_session_list(&mut self) {
        self.session_list
            .open(self.sessions.active_index(), self.sessions.len());
        self.request_recent_sessions();
        self.overlay = OverlayMode::SessionList;
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Fetch the newest agent-loop summaries for the recent pane. Failures
    /// surface as a notice; the tabs pane keeps working regardless. Rows are
    /// newest-first with server titles so the pane matches the mini `/sessions`
    /// listing and empty servers stay explicit instead of silent.
    pub(super) fn request_recent_sessions(&mut self) {
        let client = self.client.clone();
        let tx = self.data_tx.clone();
        let now_ms = self.now_ms();
        self.tasks.push(tokio::spawn(async move {
            let result: Result<Vec<crate::session_list::RecentSession>, String> = async {
                let data: serde_json::Value = client
                    .get_json(wf_cli_shared::paths::AGENT_LOOPS_SUMMARIES)
                    .await
                    .map_err(|err| err.to_string())?;
                let mut items = data
                    .get("items")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .or_else(|| data.as_array().cloned())
                    .unwrap_or_default();
                items.sort_by_key(|item| {
                    std::cmp::Reverse(item.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0))
                });
                Ok(items
                    .into_iter()
                    .take(50)
                    .filter_map(|item| {
                        let id = item.get("id")?.as_str()?.to_string();
                        let status = item
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?")
                            .to_string();
                        let start_ms = item.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0);
                        let title = item
                            .get("title")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|title| !title.is_empty())
                            .map(str::to_string);
                        Some(crate::session_list::RecentSession {
                            id,
                            status,
                            started: crate::session_list::relative_age(start_ms, now_ms),
                            title,
                        })
                    })
                    .collect())
            }
            .await;
            let _ = tx.send(DataResult::RecentSessions {
                result: result.map_err(crate::error::CliError::Business),
            });
        }));
    }

    /// Delete the selected recent session from the server (loop plus its
    /// conversation). An open tab for the same session is closed so the UI
    /// never shows a tab without server history; the recent pane refreshes.
    pub(super) async fn delete_recent_session(&mut self) {
        let Some(id) = self.session_list.recent_selected_id() else {
            self.app
                .notice
                .set("recent: nothing selected".to_string());
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        };
        let short = crate::screens::short_id(&id);
        match self.client.delete_agent_loop(&id).await {
            Ok(_) => {
                if let Some(index) = self.sessions.index_of_execution(&id) {
                    self.close_tab(index).await;
                }
                self.request_recent_sessions();
                self.app
                    .notice
                    .set(format!("deleted session {short}"));
            }
            Err(err) => {
                self.app
                    .notice
                    .set(format!("delete {short} failed: {err}"));
            }
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Rename the selected recent session to the current filter text. The
    /// filter doubles as the rename input so no extra prompt is needed:
    /// type the new title, then press Ctrl+R. Blank filters are rejected.
    pub(super) async fn rename_recent_session(&mut self) {
        let Some(id) = self.session_list.recent_selected_id() else {
            self.app
                .notice
                .set("recent: nothing selected".to_string());
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        };
        let title = self.session_list.recent_filter().trim().to_string();
        if title.is_empty() {
            self.app.notice.set(
                "type a new title first, then press Ctrl+R to rename".to_string(),
            );
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        }
        let short = crate::screens::short_id(&id);
        match self.client.rename_agent_loop(&id, &title).await {
            Ok(_) => {
                self.session_list.clear_recent_filter();
                self.request_recent_sessions();
                self.app
                    .notice
                    .set(format!("renamed {short} to {title}"));
            }
            Err(err) => {
                self.app
                    .notice
                    .set(format!("rename {short} failed: {err}"));
            }
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Open the session tree browser for the active session and fetch its
    /// model on a background task; the placeholder paints until it lands.
    pub(super) fn open_session_tree(&mut self) {
        let Some(session_id) = self
            .sessions
            .active()
            .and_then(|session| session.session_id())
            .map(str::to_owned)
        else {
            self.app.notice.set("no active session for the tree");
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        };
        self.session_tree.open(session_id.clone());
        self.session_tree
            .set_bookmarks(Self::tree_bookmark_ids(&session_id));
        self.request_session_tree();
        self.overlay = OverlayMode::SessionTree;
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Bookmarked node ids for one session from the persisted file.
    fn tree_bookmark_ids(session_id: &str) -> Vec<String> {
        crate::tab_persistence::load_bookmarks()
            .bookmarks
            .into_iter()
            .filter(|bookmark| bookmark.session_id == session_id)
            .map(|bookmark| bookmark.node_id)
            .collect()
    }

    /// Toggle the bookmark on the tree selection and persist it. Marking is
    /// client-side only: stale ids simply match nothing on reopen.
    pub(super) fn toggle_tree_bookmark(&mut self) {
        let Some(session_id) = self.session_tree.session_id().map(str::to_owned) else {
            return;
        };
        let Some((node_id, label, marked)) = self.session_tree.toggle_bookmark_selected() else {
            self.app.notice.set("tree: nothing selected".to_string());
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        };
        let mut saved = crate::tab_persistence::load_bookmarks();
        if marked {
            saved.bookmarks.push(crate::tab_persistence::SavedBookmark {
                session_id: session_id.clone(),
                node_id: node_id.clone(),
                label,
                created_ms: self.now_ms(),
            });
            self.app.notice.set(format!(
                "tree: bookmarked {}",
                crate::screens::short_id(&node_id)
            ));
        } else {
            saved.bookmarks.retain(|bookmark| {
                !(bookmark.session_id == session_id && bookmark.node_id == node_id)
            });
            self.app.notice.set(format!(
                "tree: unmarked {}",
                crate::screens::short_id(&node_id)
            ));
        }
        crate::tab_persistence::save_bookmarks(&saved);
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Branch the active session from its latest checkpoint into a new tab.
    /// Restore lands as a new commit under a fresh execution id; the source
    /// tab keeps running untouched. This is a branch, never an in-place
    /// switch, and the notice says so explicitly.
    pub(super) async fn branch_latest(&mut self) {
        let Some(source) = self
            .sessions
            .active()
            .and_then(|session| session.session_id())
            .map(str::to_owned)
        else {
            self.app.notice.set("no active session to branch");
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        };
        let target = self.branch_restore_target(&source).await;
        let Some((checkpoint_id, is_loop, _timestamp)) = target else {
            self.app.notice.set(format!(
                "no checkpoints yet in {}; send a turn first",
                crate::screens::short_id(&source)
            ));
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        };
        let restore = self
            .restore_checkpoint(&source, &checkpoint_id, is_loop)
            .await;
        match restore {
            Ok(new_id) => {
                let short_source = crate::screens::short_id(&source);
                let short_new = crate::screens::short_id(&new_id);
                let short_cp = crate::screens::short_id(&checkpoint_id);
                self.open_tab_for_session(new_id.clone(), Some(source), None)
                    .await;
                Self::mark_branch_title(self.sessions.active_tab_mut(), &new_id, &short_source);
                self.app.notice.set(format!(
                    "branched {short_source} at {short_cp} into new tab {short_new} (history preserved; branch, not in-place)"
                ));
            }
            Err(err) => {
                self.app.notice.set(format!("branch failed: {err}"));
            }
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Restore `checkpoint_id` of `source` as a new commit under a fresh
    /// execution id. History is never rewritten; the source keeps running.
    /// Returns the new execution id, trying the agent-loop endpoint first
    /// and falling back to the execution endpoint.
    async fn restore_checkpoint(
        &self,
        source: &str,
        checkpoint_id: &str,
        is_loop: bool,
    ) -> Result<String, String> {
        // Try the known domain first, then the other one: checkpoint ids
        // are unique per domain, so exactly one endpoint can serve them.
        let attempts = if is_loop {
            [true, false]
        } else {
            [false, true]
        };
        let mut last_err = format!("unknown checkpoint {checkpoint_id}");
        for attempt_loop in attempts {
            let outcome = if attempt_loop {
                self.client
                    .post_json::<_, serde_json::Value>(
                        &format!(
                            "/api/v1/agent-loops/{source}/checkpoints/{checkpoint_id}/restore"
                        ),
                        &serde_json::json!({}),
                    )
                    .await
                    .map(|value| {
                        value
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or(checkpoint_id)
                            .to_string()
                    })
                    .map_err(|err| err.to_string())
            } else {
                self.client
                    .post_json::<_, serde_json::Value>(
                        &format!("/api/v1/executions/checkpoints/{checkpoint_id}/restore"),
                        &serde_json::json!({}),
                    )
                    .await
                    .map(|value| {
                        value
                            .get("execution_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or(checkpoint_id)
                            .to_string()
                    })
                    .map_err(|err| err.to_string())
            };
            match outcome {
                Ok(new_id) => return Ok(new_id),
                Err(err) => last_err = err,
            }
        }
        Err(last_err)
    }

    /// Origin note for a branch tab: the tab strip is the visible top of
    /// the new page, so the source short id rides in the title. The title
    /// stays default-eligible, so content-based auto-naming may replace it
    /// later without any extra bookkeeping.
    fn mark_branch_title(tab: Option<&mut SessionTab>, new_id: &str, short_source: &str) {
        if let Some(tab) = tab {
            tab.title = format!(
                "{} (from {})",
                crate::screens::short_id(new_id),
                short_source
            );
        }
    }

    /// Latest checkpoint of `source`: agent-loop chain first, execution
    /// chain as fallback. Returns the checkpoint id, which domain served
    /// it, and its timestamp for branch messaging.
    async fn branch_restore_target(&self, source: &str) -> Option<(String, bool, i64)> {
        // Timestamp decides the latest checkpoint, never chain position.
        let pick_latest = |value: serde_json::Value| -> Option<(String, i64)> {
            crate::session_tree::checkpoint_points(&value)
                .into_iter()
                .map(|point| (point.id, point.timestamp))
                .max_by_key(|(_, timestamp)| *timestamp)
        };
        if let Ok(view) = self.client.agent_loop_checkpoints_chain(source).await {
            let value = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
            if let Some((id, timestamp)) = pick_latest(value) {
                return Some((id, true, timestamp));
            }
        }
        if let Ok(view) = self.client.execution_checkpoints_chain(source).await {
            let value = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
            if let Some((id, timestamp)) = pick_latest(value) {
                return Some((id, false, timestamp));
            }
        }
        None
    }

    /// Load the tree model for the open tree overlay on a background task:
    /// conversation records plus the checkpoint chain, assembled into
    /// display nodes. The newest messages win under the node cap.
    pub(super) fn request_session_tree(&mut self) {
        let Some(session_id) = self.session_tree.session_id().map(str::to_owned) else {
            return;
        };
        let client = self.client.clone();
        let tx = self.data_tx.clone();
        self.tasks.push(tokio::spawn(async move {
            let records = crate::replay::conversation_records(&client, &session_id).await;
            let total_messages = records.len();
            let chain = if let Ok(view) = client.agent_loop_checkpoints_chain(&session_id).await {
                serde_json::to_value(&view).ok()
            } else if let Ok(view) = client.execution_checkpoints_chain(&session_id).await {
                serde_json::to_value(&view).ok()
            } else {
                None
            };
            let checkpoints = chain
                .as_ref()
                .map(crate::session_tree::checkpoint_points)
                .unwrap_or_default();
            let iterations = if records.is_empty() {
                crate::replay::iteration_details(&client, &session_id).await
            } else {
                Vec::new()
            };
            let (nodes, truncated) =
                crate::session_tree::assemble_tree(&records, &checkpoints, &iterations);
            let _ = tx.send(DataResult::SessionTree {
                session_id,
                result: Ok(crate::session_tree::AssembledTree {
                    nodes,
                    truncated,
                    total_messages,
                }),
            });
        }));
    }

    /// Branch the tree selection into a new tab: restore the nearest
    /// checkpoint at-or-before the selected node, open the new execution as
    /// a child tab of the source session, and prefill user-message text so
    /// it can be edited and resent. Failures keep the tree open with its
    /// selection; success closes the tree and lands in the new tab.
    /// Resolve the file commit id linked to a state checkpoint, if any.
    async fn resolve_state_file_commit(&self, checkpoint_id: &str) -> Option<String> {
        let url = format!("/api/v1/file-checkpoint/state-links?ids={}", checkpoint_id);
        let value: serde_json::Value = self.client.get_json(&url).await.ok()?;
        let arr = value.as_array()?;
        let first = arr.first()?;
        if first.get("state").and_then(|v| v.as_str()) != Some("linked") {
            return None;
        }
        first
            .get("file_commit_id")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
    }

    pub(super) async fn branch_from_tree_with_scope(
        &mut self,
        scope: crate::session_tree::RestoreScope,
    ) {
        let Some(source) = self.session_tree.session_id().map(str::to_owned) else {
            self.app.notice.set("tree: no session to branch");
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        };
        let Some(node) = self.session_tree.selected_node().cloned() else {
            self.app.notice.set("tree: nothing selected");
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        };
        let Some(checkpoint_id) = node.checkpoint_id.clone() else {
            self.app.notice.set(
                "tree: no checkpoint at or before this point yet — send a turn first".to_string(),
            );
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        };
        let short_source = crate::screens::short_id(&source);
        let short_cp = crate::screens::short_id(&checkpoint_id);
        let mut notices: Vec<String> = Vec::new();
        let mut state_ok = true;
        let mut new_id: Option<String> = None;

        // History restore (state) unless workspace-only.
        if scope != crate::session_tree::RestoreScope::Workspace {
            match self.restore_checkpoint(&source, &checkpoint_id, true).await {
                Ok(id) => {
                    new_id = Some(id);
                    notices.push(format!(
                        "history branched {short_source} at {short_cp} into a new tab"
                    ));
                }
                Err(err) => {
                    state_ok = false;
                    self.app.notice.set(format!("branch failed: {err}"));
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return;
                }
            }
        }

        // Workspace restore unless history-only.
        if scope != crate::session_tree::RestoreScope::History {
            // Resolve the linked file commit via the state->file link.
            let file_commit = self.resolve_state_file_commit(&checkpoint_id).await;
            match file_commit {
                Some(commit) => {
                    let entity = source.clone();
                    let body = serde_json::json!({
                        "entity": entity,
                        "checkpoint": commit,
                    });
                    match self
                        .client
                        .post_json::<_, serde_json::Value>("/api/v1/file-checkpoint/restore", &body)
                        .await
                    {
                        Ok(value) => {
                            let restored =
                                value.get("restored").and_then(|v| v.as_u64()).unwrap_or(0);
                            notices.push(format!(
                                "workspace restored ({restored} file(s)) from {short_cp}"
                            ));
                        }
                        Err(err) => {
                            notices.push(format!("workspace restore failed: {err}"));
                        }
                    }
                }
                None => {
                    notices.push(format!(
                        "no linked workspace commit for {short_cp}; workspace unchanged"
                    ));
                }
            }
        }

        // Land in the new tab when state was restored.
        if let Some(id) = new_id {
            let prefill = (node.kind == crate::session_tree::TreeNodeKind::User)
                .then(|| node.full_text.clone())
                .filter(|text| !text.trim().is_empty());
            self.session_tree.close();
            self.overlay = OverlayMode::None;
            self.open_tab_for_session(id.clone(), Some(source.clone()), prefill)
                .await;
            Self::mark_branch_title(
                self.sessions.active_tab_mut(),
                &id,
                &crate::screens::short_id(&source),
            );
        } else {
            // Workspace-only: close the tree, stay on the current tab.
            self.session_tree.close();
            self.overlay = OverlayMode::None;
        }
        self.app.notice.set(notices.join("; "));
        let _ = state_ok;
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Open a new tab bound to an existing server session id (branch
    /// restores, replays, restores-from-tabs-file). The tab replays the
    /// server history on attach, matching restored-tab behavior.
    pub(super) async fn open_tab_for_session(
        &mut self,
        id: String,
        parent_id: Option<String>,
        prefill: Option<String>,
    ) {
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
        let mut session = InteractiveController::start(self.client.clone(), id.clone()).await;
        session.set_keymap(self.keymap.clone());
        session.set_perf_tier(self.perf_tier.marker());
        session.apply_perf_policy(self.perf_policy);
        session.apply_copy_policy(self.copy_policy);
        session.apply_clear_selection_after_copy(self.clear_selection_after_copy());
        session.set_tool_config(load_tool_output_config());
        session.set_session_scrollbar(load_session_scrollbar_opt_in());
        session.load_replay(&id);
        if let Some(text) = prefill {
            session.prefill_composer(&text);
        }
        let workspace = std::env::current_dir()
            .ok()
            .and_then(|dir| dir.to_str().map(str::to_string));
        let mut tab = SessionTab::new(session, id, self.now_ms());
        tab.parent_id = parent_id;
        tab.workspace = workspace;
        self.sessions.push(tab);
        self.save_tabs();
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
        session.apply_clear_selection_after_copy(self.clear_selection_after_copy());
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
