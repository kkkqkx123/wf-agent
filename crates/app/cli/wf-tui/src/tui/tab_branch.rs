//! Branch and session-tree tabs for the TUI shell.
//!
//! Branches restore checkpoints into new tabs so the source tab keeps
//! running untouched; the tree browser only reads history and bookmarks.

use crate::overlay::OverlayMode;
use crate::session_tabs::SessionTab;

use super::data_cache::DataResult;
use super::TuiApp;

impl TuiApp {
    /// Open the session tree browser for the active session and fetch its
    /// model on a background task; the placeholder paints until it lands.
    pub(crate) fn open_session_tree(&mut self) {
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
    pub(crate) fn tree_bookmark_ids(session_id: &str) -> Vec<String> {
        crate::tab_persistence::load_bookmarks()
            .bookmarks
            .into_iter()
            .filter(|bookmark| bookmark.session_id == session_id)
            .map(|bookmark| bookmark.node_id)
            .collect()
    }

    /// Toggle the bookmark on the tree selection and persist it. Marking is
    /// client-side only: stale ids simply match nothing on reopen.
    pub(crate) fn toggle_tree_bookmark(&mut self) {
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
    pub(crate) async fn branch_latest(&mut self) {
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
    pub(crate) async fn restore_checkpoint(
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
    pub(crate) fn mark_branch_title(
        tab: Option<&mut SessionTab>,
        new_id: &str,
        short_source: &str,
    ) {
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
    pub(crate) async fn branch_restore_target(&self, source: &str) -> Option<(String, bool, i64)> {
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
    pub(crate) fn request_session_tree(&mut self) {
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
    pub(crate) async fn resolve_state_file_commit(&self, checkpoint_id: &str) -> Option<String> {
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

    pub(crate) async fn branch_from_tree_with_scope(
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
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }
}
