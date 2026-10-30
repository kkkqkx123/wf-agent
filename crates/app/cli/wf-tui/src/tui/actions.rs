//! Destructive / mutating actions dispatched from key routes: delete
//! workflow, pause/resume and cancel executions, default model selection,
//! checkpoint detail/chain/restore/delete/create/purge/undo-last-input,
//! file-workspace submit/merge/undo/redo/gc/reindex, edit-session
//! begin/rollback.

use crate::modal::{ConfirmModal, ModalResult, ModelPicker};
use crate::overlay::Feedback;
use crate::screens::{short_id, ApprovalRow, CheckpointRow, ConflictRow, ScreenData, ScreenKind};
use wf_client::WfClient;

use super::data_cache::DataResult;
use super::TuiApp;

impl TuiApp {
    /// Currently selected checkpoint row, or `None` off the screen.
    fn selected_checkpoint_row(&self) -> Option<CheckpointRow> {
        let ScreenData::Checkpoints(rows) = self.display_data() else {
            return None;
        };
        let idx = self
            .app
            .screen
            .navigation
            .selected()
            .min(rows.len().saturating_sub(1));
        rows.get(idx).cloned()
    }

    /// Open the read-only detail viewer for the selected checkpoint.
    pub(super) fn open_selected_checkpoint_detail(&mut self) {
        let Some(row) = self.selected_checkpoint_row() else {
            return;
        };
        let id = row.id.clone();
        let task_id = id.clone();
        let client = self.client.clone();
        let tx = self.data_tx.clone();
        self.tasks.push(tokio::spawn(async move {
            let result = client
                .get_json::<serde_json::Value>(&format!("/api/v1/checkpoints/{task_id}"))
                .await
                .and_then(|value| {
                    serde_json::from_value::<wf_types::storage::checkpoint::Checkpoint>(value)
                        .map_err(|e| {
                            wf_client::ClientError::Invalid(format!("checkpoint decode: {e}"))
                        })
                })
                .map(|checkpoint| crate::fetch::format_checkpoint_detail(&checkpoint))
                .map_err(crate::error::CliError::from);
            let _ = tx.send(DataResult::CheckpointDetail {
                title: format!("Checkpoint {}", short_id(&task_id)),
                result,
            });
        }));
        self.app
            .notice
            .set(format!("Loading checkpoint {}...", short_id(&id)));
    }

    /// Open the read-only chain viewer for the selected row's execution,
    /// routing to the agent-loop or workflow chain endpoint by entity type.
    pub(super) fn open_selected_checkpoint_chain(&mut self) {
        let Some(row) = self.selected_checkpoint_row() else {
            return;
        };
        let entity = row.entity.clone();
        let is_loop = row.entity_type == "agent_loop";
        let task_entity = entity.clone();
        let client = self.client.clone();
        let tx = self.data_tx.clone();
        self.tasks.push(tokio::spawn(async move {
            // Typed chain read; the loop route returns the capped item view
            // while the execution route returns the chain analysis shape.
            // Both convert back to JSON for the read-only chain viewer.
            let result = if is_loop {
                client
                    .agent_loop_checkpoints_chain(&task_entity)
                    .await
                    .map(|view| {
                        crate::fetch::format_checkpoint_chain(
                            &serde_json::to_value(&view).unwrap_or(serde_json::Value::Null),
                        )
                    })
                    .map_err(crate::error::CliError::from)
            } else {
                client
                    .execution_checkpoints_chain(&task_entity)
                    .await
                    .map(|view| {
                        crate::fetch::format_checkpoint_chain(
                            &serde_json::to_value(&view).unwrap_or(serde_json::Value::Null),
                        )
                    })
                    .map_err(crate::error::CliError::from)
            };
            let _ = tx.send(DataResult::CheckpointChain {
                title: format!("Chain {}", short_id(&task_entity)),
                result,
            });
        }));
        self.app
            .notice
            .set(format!("Loading chain {}...", short_id(&entity)));
    }

    /// Restore the selected checkpoint after confirmation. Restore lands as
    /// a new commit under a new execution; history is never rewritten.
    /// Agent-loop rows restore through the loop endpoint, workflow rows
    /// through the executions endpoint.
    pub(super) fn restore_selected_checkpoint(&mut self) {
        let Some(row) = self.selected_checkpoint_row() else {
            return;
        };
        let id = row.id.clone();
        let entity = row.entity.clone();
        let is_loop = row.entity_type == "agent_loop";
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Restore checkpoint",
            format!(
                "Restore {} as a new commit? History is not rewritten.",
                short_id(&id)
            ),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let result: Result<String, String> = if is_loop {
                        client
                            .post_json::<_, serde_json::Value>(
                                &format!("/api/v1/agent-loops/{entity}/checkpoints/{id}/restore"),
                                &serde_json::json!({}),
                            )
                            .await
                            .map(|value| {
                                value
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&id)
                                    .to_string()
                            })
                            .map_err(|err| err.to_string())
                    } else {
                        client
                            .post_json::<_, serde_json::Value>(
                                &format!("/api/v1/executions/checkpoints/{id}/restore"),
                                &serde_json::json!({}),
                            )
                            .await
                            .map(|value| {
                                value
                                    .get("execution_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&id)
                                    .to_string()
                            })
                            .map_err(|err| err.to_string())
                    };
                    match result {
                        Ok(new_id) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Dashboard));
                            format!("Restored {} as {new_id}", short_id(&id))
                        }
                        Err(err) => format!("Restore failed: {err}"),
                    }
                }
                _ => "Restore cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Delete the selected checkpoint after confirmation.
    pub(super) fn delete_selected_checkpoint(&mut self) {
        let Some(row) = self.selected_checkpoint_row() else {
            return;
        };
        let id = row.id.clone();
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Delete checkpoint",
            format!(
                "Delete checkpoint {}? This cannot be undone.",
                short_id(&id)
            ),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    match client
                        .delete_json::<bool>(&format!("/api/v1/checkpoints/{id}"))
                        .await
                    {
                        Ok(true) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Dashboard));
                            let _ = tx.send(Feedback::Notice(format!(
                                "Deleted checkpoint {}",
                                short_id(&id)
                            )));
                        }
                        Ok(false) => {
                            let _ = tx.send(Feedback::Notice(format!(
                                "Checkpoint {} not found",
                                short_id(&id)
                            )));
                        }
                        Err(err) => {
                            let _ = tx.send(Feedback::Notice(format!("Delete failed: {err}")));
                        }
                    }
                    return;
                }
                _ => "Delete cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Open the read-only detail viewer for the selected row of a screen
    /// whose rows pre-render their viewer text at fetch time.
    pub(super) fn open_selected_cached_detail(&mut self) {
        let selected = self.app.screen.navigation.selected();
        let detail = match self.display_data() {
            ScreenData::Catalog(rows) => {
                let idx = selected.min(rows.len().saturating_sub(1));
                rows.get(idx).map(|row| {
                    (
                        format!("Catalog {} {}", row.kind.label(), short_id(&row.id)),
                        row.detail.clone(),
                    )
                })
            }
            ScreenData::Git(rows) => {
                let idx = selected.min(rows.len().saturating_sub(1));
                rows.get(idx)
                    .map(|row| (format!("Git {}", row.path), row.detail.clone()))
            }
            ScreenData::FileWorkspace(rows) => {
                let idx = selected.min(rows.len().saturating_sub(1));
                rows.get(idx)
                    .map(|row| (format!("File {}", row.path), row.detail.clone()))
            }
            ScreenData::FileTimeline(data) => {
                let idx = selected.min(data.rows.len().saturating_sub(1));
                data.rows
                    .get(idx)
                    .map(|row| (format!("Timeline {}", row.path), row.detail.clone()))
            }
            ScreenData::FileSync(data) => {
                let idx = selected.min(data.rows.len().saturating_sub(1));
                data.rows
                    .get(idx)
                    .map(|row| (format!("Sync {}", row.title), row.detail.clone()))
            }
            ScreenData::EditSessions(rows) => {
                let idx = selected.min(rows.len().saturating_sub(1));
                rows.get(idx)
                    .map(|row| (format!("Session {}", short_id(&row.id)), row.detail.clone()))
            }
            ScreenData::FileApprovals(rows) => {
                let idx = selected.min(rows.len().saturating_sub(1));
                rows.get(idx).map(|row| {
                    (
                        format!("Approval {}", short_id(&row.actor)),
                        row.detail.clone(),
                    )
                })
            }
            ScreenData::Conflicts(rows) => {
                let idx = selected.min(rows.len().saturating_sub(1));
                rows.get(idx)
                    .map(|row| (format!("Conflict {}", row.path), row.detail.clone()))
            }
            _ => None,
        };
        if let Some((title, text)) = detail {
            self.modals
                .push(Box::new(crate::file_viewer::FileViewer::new(title, &text)));
        }
    }

    /// Open the read-only version timeline for the file selected on the Files
    /// screen. The path is captured before navigating so the timeline fetch
    /// knows which file to follow; without a selection the notice explains it.
    pub(super) fn open_selected_file_timeline(&mut self) {
        let selected = self.app.screen.navigation.selected();
        let path = match self.display_data() {
            ScreenData::FileWorkspace(rows) => {
                let idx = selected.min(rows.len().saturating_sub(1));
                rows.get(idx).map(|row| row.path.clone())
            }
            _ => None,
        };
        let Some(path) = path else {
            self.app
                .notice
                .set("No file selected for a timeline.".to_string());
            return;
        };
        self.app.screen.file_timeline_path = Some(path);
        self.invalidate(ScreenKind::FileTimeline);
        self.goto(ScreenKind::FileTimeline);
    }

    /// Open the git-sync screen from the Files screen. The screen fetches
    /// its three read-only sections on mount; no file selection is needed.
    pub(super) fn open_file_sync(&mut self) {
        self.invalidate(ScreenKind::FileSync);
        self.goto(ScreenKind::FileSync);
    }

    /// Effective auto-commit default behind the sync screen, for the export
    /// request body. Off the sync screen there is nothing to export.
    fn file_sync_auto_commit(&self) -> Option<bool> {
        match self.display_data() {
            ScreenData::FileSync(data) => Some(data.auto_commit),
            _ => None,
        }
    }

    /// Export the main tree into the user workspace after confirmation. The
    /// confirmation is the overwrite guard; auto-commit rides the cached
    /// sync default and the message is server-suggested.
    pub(super) fn export_file_sync(&mut self) {
        let Some(auto_commit) = self.file_sync_auto_commit() else {
            self.app
                .notice
                .set("Open the sync screen before exporting.".to_string());
            return;
        };
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Export to workspace",
            format!(
                "Export main to the workspace? This overwrites worktree changes. auto commit {auto}.",
                auto = if auto_commit { "on" } else { "off" }
            ),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    match client
                        .post_json::<_, serde_json::Value>(
                            "/api/v1/file-checkpoint/sync/export",
                            &serde_json::json!({
                                "confirm_overwrite": true,
                                "auto_commit": auto_commit,
                                "message": null,
                            }),
                        )
                        .await
                    {
                        Ok(value) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::FileSync));
                            let commit = value
                                .get("reconcile")
                                .and_then(|r| r.get("internal_main"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let idempotent = value
                                .get("idempotent")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            match (commit.is_empty(), idempotent) {
                                (true, _) => "Export completed".to_string(),
                                (false, true) => {
                                    format!("Export already up to date ({})", short_id(commit))
                                }
                                (false, false) => {
                                    format!("Exported main ({})", short_id(commit))
                                }
                            }
                        }
                        Err(err) => sync_failure_notice("Export", &err),
                    }
                }
                _ => "Export cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Import the external head as human history. The endpoint is idempotent,
    /// so this runs directly with no extra prompt.
    pub(super) fn import_file_sync(&mut self) {
        if self.file_sync_auto_commit().is_none() {
            self.app
                .notice
                .set("Open the sync screen before importing.".to_string());
            return;
        }
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match client
                .post_json::<_, serde_json::Value>(
                    "/api/v1/file-checkpoint/sync/import",
                    &serde_json::json!({}),
                )
                .await
            {
                Ok(value) => {
                    let _ = tx.send(Feedback::Refresh(ScreenKind::FileSync));
                    let advanced = value
                        .get("advanced_to")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let files = value
                        .get("file_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let idempotent = value
                        .get("idempotent")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if idempotent {
                        "Import already up to date".to_string()
                    } else {
                        format!("Imported {files} file(s) ({})", short_id(advanced))
                    }
                }
                Err(err) => sync_failure_notice("Import", &err),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Prompt for the rebase baseline selector, then hand the parsed selector
    /// to the main loop for its destructive confirmation. The prompt answer
    /// arrives on a background task that cannot push modals, so the confirm
    /// step travels back as feedback.
    pub(super) fn rebase_file_sync(&mut self) {
        if self.file_sync_auto_commit().is_none() {
            self.app
                .notice
                .set("Open the sync screen before rebasing.".to_string());
            return;
        }
        let rx = self
            .modals
            .push_with_result(Box::new(crate::prompt_modal::PromptModal::new(
                "Rebase baseline",
                "branch | prefix=<p> | behind=<n> | mode=<borrow|copy>",
            )));
        let tx = self.feedback_tx.clone();
        self.tasks.push(tokio::spawn(async move {
            match rx.await {
                Ok(ModalResult::Value(selector)) => {
                    let _ = tx.send(Feedback::RebaseConfirm(selector));
                }
                _ => {
                    let _ = tx.send(Feedback::Notice("Rebase cancelled".to_string()));
                }
            }
        }));
    }

    /// Destructive confirmation for the parsed rebase selector, pushed on
    /// the main loop so the write still gates behind an explicit `[y]`.
    pub(super) fn confirm_file_sync_rebase(&mut self, selector: &str) {
        let params = match parse_rebase_selector(selector) {
            Ok(params) => params,
            Err(err) => {
                self.app.notice.set(err);
                return;
            }
        };
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Rebase baseline",
            format!(
                "Move the baseline ({})? Irreversible: history is rewritten.",
                params.describe()
            ),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    match client
                        .post_json::<_, serde_json::Value>(
                            "/api/v1/file-checkpoint/sync/rebase",
                            &serde_json::json!({
                                "branch": params.branch,
                                "commit_prefix": params.commit_prefix,
                                "behind": params.behind,
                                "mode": params.mode,
                                "confirm": true,
                            }),
                        )
                        .await
                    {
                        Ok(value) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::FileSync));
                            let anchor = value
                                .get("new_anchor")
                                .and_then(|a| a.get("id"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            format!("Baseline rebased ({})", short_id(anchor))
                        }
                        Err(err) => sync_failure_notice("Rebase", &err),
                    }
                }
                _ => "Rebase cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Open the full-text viewer for the selected execution subview.
    /// Overview keeps its replay jump; other views show the cached detail
    /// lines and refetch when they are missing.
    pub(super) fn open_selected_exec_viewer(&mut self) {
        if self.app.screen.exec_view == crate::screens::ExecView::Overview {
            return;
        }
        let Some((key, _)) = self.current_detail_key() else {
            return;
        };
        let cached = self
            .app
            .screen
            .detail_for(&key)
            .map(|c| (c.title.clone(), c.lines.join("\n")));
        if let Some((title, text)) = cached {
            self.modals
                .push(Box::new(crate::file_viewer::FileViewer::new(title, &text)));
        } else {
            self.app.notice.set("Loading detail...".to_string());
            self.request_detail_for_current();
        }
    }

    /// Open the full-text viewer for the selected workflow subview.
    /// Summary renders from the cached row; other views use fetched lines.
    pub(super) fn open_selected_workflow_viewer(&mut self) {
        if self.app.screen.workflow_view == crate::screens::WorkflowView::Summary {
            let rows = match self.display_data() {
                ScreenData::Workflow(rows) => rows,
                _ => return,
            };
            let selected = self.app.screen.navigation.selected();
            let Some(row) = rows.get(selected.min(rows.len().saturating_sub(1))) else {
                return;
            };
            let text = format!(
                "id: {}\nname: {}\ndescription: {}\nnodes: {}",
                row.id,
                row.name,
                row.description.as_deref().unwrap_or("-"),
                row.node_count
            );
            self.modals
                .push(Box::new(crate::file_viewer::FileViewer::new(
                    format!("Workflow {}", row.name),
                    &text,
                )));
            return;
        }
        let Some((key, _)) = self.current_detail_key() else {
            return;
        };
        let cached = self
            .app
            .screen
            .detail_for(&key)
            .map(|c| (c.title.clone(), c.lines.join("\n")));
        if let Some((title, text)) = cached {
            self.modals
                .push(Box::new(crate::file_viewer::FileViewer::new(title, &text)));
        } else {
            self.app.notice.set("Loading detail...".to_string());
            self.request_detail_for_current();
        }
    }

    /// Prompt for a client-side substring filter over the current list.
    /// Clearing goes through the `X` key: the prompt never submits empty.
    pub(super) fn prompt_list_filter(&mut self) {
        let rx = self
            .modals
            .push_with_result(Box::new(crate::prompt_modal::PromptModal::new(
                "Filter list",
                "Show rows containing",
            )));
        let tx = self.feedback_tx.clone();
        self.tasks.push(tokio::spawn(async move {
            if let Ok(ModalResult::Value(text)) = rx.await {
                let _ = tx.send(Feedback::Filter(text.trim().to_string()));
            }
        }));
    }

    /// Apply `apply` to the local config file and persist it. Runtime
    /// policies resolve at startup, so the notice states the restart
    /// requirement instead of implying an instant effect.
    fn update_local_config(
        &mut self,
        apply: impl FnOnce(&mut crate::app_config::AppConfig) -> String,
    ) {
        let Some(path) = crate::app_config::config_file_path() else {
            self.app.notice.set("Config path unavailable.".to_string());
            return;
        };
        let mut config = crate::app_config::ConfigManager::load(&path)
            .map(|manager| manager.config().clone())
            .unwrap_or_default();
        let label = apply(&mut config);
        match crate::app_config::ConfigManager::save(&path, &config) {
            Ok(()) => {
                self.invalidate(ScreenKind::Settings);
                self.app.notice.set(format!("{label} (restart applies)"));
            }
            Err(err) => self.app.notice.set(format!("Save failed: {err}")),
        }
    }

    pub(super) fn toggle_mouse_capture(&mut self) {
        let Some(path) = crate::app_config::config_file_path() else {
            self.app.notice.set("Config path unavailable.".to_string());
            return;
        };
        let mut config = crate::app_config::ConfigManager::load(&path)
            .map(|manager| manager.config().clone())
            .unwrap_or_default();
        config.behavior.mouse_capture = !config.behavior.mouse_capture;
        let enabled = config.behavior.mouse_capture;
        match crate::app_config::ConfigManager::save(&path, &config) {
            Ok(()) => {
                self.invalidate(ScreenKind::Settings);
                self.perf_policy.enable_mouse_capture = enabled;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                self.app.notice.set(format!(
                    "mouse_capture {} (applies instantly)",
                    if enabled { "on" } else { "off" }
                ));
            }
            Err(err) => self.app.notice.set(format!("Save failed: {err}")),
        }
    }

    pub(crate) fn clear_selection_after_copy(&self) -> bool {
        matches!(
            self.copy_on_select,
            crate::app_config::CopyOnSelectMode::CopyAndClear
        )
    }

    pub(crate) fn copy_on_select_enabled(&self) -> bool {
        !matches!(
            self.copy_on_select,
            crate::app_config::CopyOnSelectMode::Off
        )
    }

    pub(super) fn toggle_copy_on_select(&mut self) {
        let Some(path) = crate::app_config::config_file_path() else {
            self.app.notice.set("Config path unavailable.".to_string());
            return;
        };
        let mut config = crate::app_config::ConfigManager::load(&path)
            .map(|manager| manager.config().clone())
            .unwrap_or_default();
        use crate::app_config::CopyOnSelectMode;
        config.behavior.copy_on_select = match config.behavior.copy_on_select {
            CopyOnSelectMode::Off => CopyOnSelectMode::Copy,
            CopyOnSelectMode::Copy => CopyOnSelectMode::CopyAndClear,
            CopyOnSelectMode::CopyAndClear => CopyOnSelectMode::Off,
        };
        let mode = config.behavior.copy_on_select;
        match crate::app_config::ConfigManager::save(&path, &config) {
            Ok(()) => {
                self.invalidate(ScreenKind::Settings);
                self.copy_on_select = mode;
                let clear = self.clear_selection_after_copy();
                for tab in self.sessions.iter_mut() {
                    tab.controller.apply_clear_selection_after_copy(clear);
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                self.app
                    .notice
                    .set(format!("copy_on_select {mode:?} (applies instantly)"));
            }
            Err(err) => self.app.notice.set(format!("Save failed: {err}")),
        }
    }

    pub(super) fn toggle_session_scrollbar(&mut self) {
        let Some(path) = crate::app_config::config_file_path() else {
            self.app.notice.set("Config path unavailable.".to_string());
            return;
        };
        let mut config = crate::app_config::ConfigManager::load(&path)
            .map(|manager| manager.config().clone())
            .unwrap_or_default();
        config.behavior.session_scrollbar = !config.behavior.session_scrollbar;
        let enabled = config.behavior.session_scrollbar;
        match crate::app_config::ConfigManager::save(&path, &config) {
            Ok(()) => {
                self.invalidate(ScreenKind::Settings);
                if let Some(session) = self.sessions.active_mut() {
                    session.set_session_scrollbar(enabled);
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                self.app.notice.set(format!(
                    "session_scrollbar {} (applies instantly)",
                    if enabled { "on" } else { "off" }
                ));
            }
            Err(err) => self.app.notice.set(format!("Save failed: {err}")),
        }
    }

    pub(super) fn toggle_sidebar(&mut self) {
        let Some(path) = crate::app_config::config_file_path() else {
            self.app.notice.set("Config path unavailable.".to_string());
            return;
        };
        let mut config = crate::app_config::ConfigManager::load(&path)
            .map(|manager| manager.config().clone())
            .unwrap_or_default();
        config.behavior.sidebar = !config.behavior.sidebar;
        let enabled = config.behavior.sidebar;
        match crate::app_config::ConfigManager::save(&path, &config) {
            Ok(()) => {
                self.invalidate(ScreenKind::Settings);
                self.sidebar = enabled;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                self.app.notice.set(format!(
                    "sidebar {} (applies instantly)",
                    if enabled { "on" } else { "off" }
                ));
            }
            Err(err) => self.app.notice.set(format!("Save failed: {err}")),
        }
    }

    /// Cycle the double-Esc action: tree -> fork -> off -> tree. Applies
    /// instantly because sessions read the config on every Esc press.
    pub(super) fn cycle_double_escape(&mut self) {
        use crate::app_config::DoubleEscapeActionName;
        self.update_local_config(|config| {
            let next = match config.behavior.double_escape_action() {
                DoubleEscapeActionName::Tree => DoubleEscapeActionName::Fork,
                DoubleEscapeActionName::Fork => DoubleEscapeActionName::Off,
                DoubleEscapeActionName::Off => DoubleEscapeActionName::Tree,
            };
            config.behavior.double_escape_action = Some(next);
            let label = match next {
                DoubleEscapeActionName::Tree => "tree",
                DoubleEscapeActionName::Fork => "fork",
                DoubleEscapeActionName::Off => "off",
            };
            format!("double-esc {label} (empty-composer double-Esc)")
        });
    }

    pub(super) fn cycle_copy_target(&mut self) {
        use crate::app_config::CopyTargetName;
        self.update_local_config(|config| {
            let next = match config.terminal.copy_target {
                Some(CopyTargetName::Code) => None,
                _ => Some(CopyTargetName::Code),
            };
            config.terminal.copy_target = next;
            format!(
                "copy_target {}",
                if next.is_some() { "code" } else { "message" }
            )
        });
    }

    pub(super) fn toggle_tool_output(&mut self) {
        self.update_local_config(|config| {
            let on = !config.tool_output.enabled.unwrap_or(true);
            config.tool_output.enabled = Some(on);
            format!("tool_output {}", if on { "on" } else { "off" })
        });
    }

    pub(super) fn cycle_tool_level(&mut self) {
        use crate::app_config::ToolDetailLevelName;
        self.update_local_config(|config| {
            let next = match config.tool_output.level {
                Some(ToolDetailLevelName::Low) => ToolDetailLevelName::Medium,
                Some(ToolDetailLevelName::Medium) => ToolDetailLevelName::High,
                _ => ToolDetailLevelName::Low,
            };
            config.tool_output.level = Some(next);
            let label = match next {
                ToolDetailLevelName::Low => "low",
                ToolDetailLevelName::Medium => "medium",
                ToolDetailLevelName::High => "high",
            };
            format!("tool_output level {label}")
        });
    }

    pub(super) fn step_preview_budget(&mut self, up: bool) {
        self.update_local_config(|config| {
            let lines = step_option(
                config.tool_output.preview_max_lines.unwrap_or(10),
                &[5, 10, 20, 40],
                up,
            );
            let chars = step_option(
                config.tool_output.preview_max_chars.unwrap_or(320),
                &[160, 320, 640, 1280],
                up,
            );
            config.tool_output.preview_max_lines = Some(lines);
            config.tool_output.preview_max_chars = Some(chars);
            format!("preview {lines} lines · {chars} chars")
        });
    }

    /// Spawn a delete after the user confirms the currently selected workflow.
    pub(super) fn delete_selected_workflow(&mut self) {
        let rows = match self.display_data() {
            ScreenData::Workflow(rows) => rows,
            _ => return,
        };
        let idx = self
            .app
            .screen
            .navigation
            .selected()
            .min(rows.len().saturating_sub(1));
        let Some(row) = rows.get(idx) else { return };
        let id = row.id.clone();
        let name = row.name.clone();
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Delete workflow",
            format!("Delete \"{name}\"? This cannot be undone."),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    match client
                        .delete_json::<bool>(&format!("/api/v1/workflows/{id}"))
                        .await
                    {
                        Ok(true) => {
                            let _ = tx.send(Feedback::Notice(format!("Deleted workflow {id}")));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Workflow));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Dashboard));
                        }
                        Ok(false) => {
                            let _ = tx.send(Feedback::Notice(format!("Workflow {id} not found")));
                        }
                        Err(err) => {
                            let _ = tx.send(Feedback::Notice(format!("Delete failed: {err}")));
                        }
                    }
                    return;
                }
                _ => "Delete cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Pause or resume the currently selected execution based on its status.
    pub(super) fn toggle_selected_execution(&mut self) {
        let rows = match self.display_data() {
            ScreenData::Executions(rows) => rows,
            _ => return,
        };
        let idx = self
            .app
            .screen
            .navigation
            .selected()
            .min(rows.len().saturating_sub(1));
        let Some(row) = rows.get(idx) else { return };
        let id = row.id.clone();
        let paused = row.status.eq_ignore_ascii_case("paused");
        let action = if paused { "resume" } else { "pause" };
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let result = toggle_execution(&client, &id, paused).await;
            let notice = match result {
                Ok(()) => {
                    let _ = tx.send(Feedback::Refresh(ScreenKind::Executions));
                    format!("Execution {id} {action}d")
                }
                Err(err) => format!("{action} failed: {err}"),
            };
            let _ = tx.send(Feedback::Notice(notice));
        }));
    }

    /// Cancel the currently selected execution after user confirmation.
    pub(super) fn cancel_selected_execution(&mut self) {
        let rows = match self.display_data() {
            ScreenData::Executions(rows) => rows,
            _ => return,
        };
        let idx = self
            .app
            .screen
            .navigation
            .selected()
            .min(rows.len().saturating_sub(1));
        let Some(row) = rows.get(idx) else { return };
        let id = row.id.clone();
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Cancel execution",
            format!("Cancel execution {id}? This cannot be undone."),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => match cancel_execution(&client, &id).await {
                    Ok(()) => {
                        let _ = tx.send(Feedback::Refresh(ScreenKind::Executions));
                        format!("Cancelled execution {id}")
                    }
                    Err(err) => format!("Cancel failed: {err}"),
                },
                _ => "Cancel dismissed".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Let the user pick a default LLM profile and apply it.
    pub(super) fn pick_default_model(&mut self) {
        let data = match self.current_data() {
            ScreenData::Settings(d) => d,
            _ => return,
        };
        if data.profiles.is_empty() {
            self.app.notice.set("No LLM profiles configured.");
            return;
        }
        let current_default = data.default_profile.clone();
        let choices: Vec<(String, String)> = data
            .profiles
            .iter()
            .map(|p| {
                let marker = if current_default.as_deref() == Some(p.id.as_str()) {
                    " *"
                } else {
                    ""
                };
                (format!("{} · {}{}", p.name, p.model, marker), p.id.clone())
            })
            .collect();
        let rx = self
            .modals
            .push_with_result(Box::new(ModelPicker::new(choices)));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let result = match rx.await {
                Ok(ModalResult::Value(id)) => {
                    match client
                        .post_json::<_, serde_json::Value>(
                            &format!("/api/v1/llm/profiles/{id}/default"),
                            &serde_json::json!({}),
                        )
                        .await
                    {
                        Ok(_) => {
                            let _ = tx.send(Feedback::Notice(format!("Default model set to {id}")));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Settings));
                        }
                        Err(err) => {
                            let _ = tx.send(Feedback::Notice(format!("Set default failed: {err}")));
                        }
                    }
                    return;
                }
                Ok(_) => "Model change cancelled".to_string(),
                Err(_) => "Model change cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(result));
        }));
    }

    /// Selected pending approval row, or `None` off the screen.
    fn selected_approval_row(&self) -> Option<ApprovalRow> {
        let ScreenData::FileApprovals(rows) = self.display_data() else {
            return None;
        };
        let idx = self
            .app
            .screen
            .navigation
            .selected()
            .min(rows.len().saturating_sub(1));
        rows.get(idx).cloned()
    }

    /// Approve the selected pending file changes after confirmation. The
    /// actor partition doubles as the approval id.
    pub(super) fn approve_selected_approval(&mut self) {
        let Some(row) = self.selected_approval_row() else {
            self.app
                .notice
                .set("No pending approval selected.".to_string());
            return;
        };
        let actor = row.actor.clone();
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Approve changes",
            format!(
                "Approve {} file(s) from {}? The change merges into its feature ref.",
                row.files, actor
            ),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let path = format!(
                        "/api/v1/file-checkpoint/approvals/{}/approve",
                        crate::fetch::common::encode_segment(&actor)
                    );
                    match client
                        .post_json::<_, serde_json::Value>(&path, &serde_json::json!({}))
                        .await
                    {
                        Ok(_) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::FileApprovals));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                            format!("Approved {actor}")
                        }
                        Err(err) => format!("Approve failed: {err}"),
                    }
                }
                _ => "Approve cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Reject the selected pending file changes after confirmation.
    pub(super) fn reject_selected_approval(&mut self) {
        let Some(row) = self.selected_approval_row() else {
            self.app
                .notice
                .set("No pending approval selected.".to_string());
            return;
        };
        let actor = row.actor.clone();
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Reject changes",
            format!(
                "Reject {} file(s) from {}? The pending review ref is removed; history is untouched.",
                row.files, actor
            ),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let path = format!(
                        "/api/v1/file-checkpoint/approvals/{}/reject",
                        crate::fetch::common::encode_segment(&actor)
                    );
                    match client
                        .post_json::<_, serde_json::Value>(
                            &path,
                            &serde_json::json!({ "reason": null }),
                        )
                        .await
                    {
                        Ok(_) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::FileApprovals));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                            format!("Rejected {actor}")
                        }
                        Err(err) => format!("Reject failed: {err}"),
                    }
                }
                _ => "Reject cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Create a checkpoint for the selected row's execution, routing to the
    /// agent-loop or workflow endpoint by the row's entity type.
    pub(super) fn create_checkpoint_for_selected(&mut self) {
        let Some(row) = self.selected_checkpoint_row() else {
            self.app
                .notice
                .set("No checkpoint row selected.".to_string());
            return;
        };
        let entity = row.entity.clone();
        let is_loop = row.entity_type == "agent_loop";
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Create checkpoint",
            format!("Create a checkpoint for {}?", short_id(&entity)),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let path = if is_loop {
                        format!("/api/v1/agent-loops/{entity}/checkpoints")
                    } else {
                        format!("/api/v1/executions/{entity}/checkpoints")
                    };
                    match client
                        .post_json::<_, serde_json::Value>(&path, &serde_json::json!({}))
                        .await
                    {
                        Ok(_) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Dashboard));
                            format!("Created checkpoint for {}", short_id(&entity))
                        }
                        Err(err) => format!("Create failed: {err}"),
                    }
                }
                _ => "Create cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Delete every checkpoint of the selected row's entity after
    /// confirmation, scoped by the row's entity type.
    pub(super) fn delete_checkpoints_for_selected_entity(&mut self) {
        let Some(row) = self.selected_checkpoint_row() else {
            self.app
                .notice
                .set("No checkpoint row selected.".to_string());
            return;
        };
        let entity = row.entity.clone();
        let entity_type = if row.entity_type == "agent_loop" {
            "agent_loop"
        } else {
            "checkpoint"
        };
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Delete entity checkpoints",
            format!(
                "Delete every checkpoint of {}? This cannot be undone.",
                short_id(&entity)
            ),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let path = format!(
                        "/api/v1/checkpoints/entity/{}?entity_type={entity_type}",
                        crate::fetch::common::encode_segment(&entity)
                    );
                    match client.delete_json::<serde_json::Value>(&path).await {
                        Ok(removed) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Dashboard));
                            format!(
                                "Deleted {} checkpoint(s) for {}",
                                removed.as_u64().unwrap_or(0),
                                short_id(&entity)
                            )
                        }
                        Err(err) => format!("Delete failed: {err}"),
                    }
                }
                _ => "Delete cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Actor partition behind the file-workspace screen: the selected row's
    /// actor, or the active session's `agent:{id}` when the list is empty.
    fn file_workspace_actor(&self) -> Option<String> {
        if let ScreenData::FileWorkspace(rows) = self.display_data() {
            let idx = self
                .app
                .screen
                .navigation
                .selected()
                .min(rows.len().saturating_sub(1));
            if let Some(actor) = rows
                .get(idx)
                .map(|row| row.actor.clone())
                .filter(|actor| !actor.is_empty())
            {
                return Some(actor);
            }
        }
        self.sessions
            .active()
            .and_then(|session| session.session_id())
            .map(|id| format!("agent:{id}"))
    }

    /// Submit the active file workspace for review after confirmation.
    pub(super) fn submit_file_workspace_for_review(&mut self) {
        let Some(actor) = self.file_workspace_actor() else {
            self.app
                .notice
                .set("No file workspace to submit.".to_string());
            return;
        };
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Submit for review",
            format!("Submit {actor} edits to a review ref? This does not merge."),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    match client
                        .post_json::<_, serde_json::Value>(
                            "/api/v1/file-checkpoint/submit",
                            &serde_json::json!({ "actor": actor }),
                        )
                        .await
                    {
                        Ok(value) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::FileApprovals));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                            let commit = value
                                .get("review_commit")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            format!("Submitted {actor} for review ({})", short_id(commit))
                        }
                        Err(err) => format!("Submit failed: {err}"),
                    }
                }
                _ => "Submit cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Merge the active file workspace into a named feature ref. The feature
    /// name is prompted; the merge outcome reports conflict files.
    pub(super) fn merge_file_workspace_to_feature(&mut self) {
        let Some(actor) = self.file_workspace_actor() else {
            self.app
                .notice
                .set("No file workspace to merge.".to_string());
            return;
        };
        let rx = self
            .modals
            .push_with_result(Box::new(crate::prompt_modal::PromptModal::new(
                "Merge to feature",
                "Feature name",
            )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Value(feature)) => {
                    match client
                        .post_json::<_, serde_json::Value>(
                            "/api/v1/file-checkpoint/merge-to-feature",
                            &serde_json::json!({ "actor": actor, "feature": feature }),
                        )
                        .await
                    {
                        Ok(value) => {
                            let conflicts = value
                                .get("conflict_files")
                                .and_then(|v| v.as_array())
                                .map(|files| files.len())
                                .unwrap_or(0);
                            let _ = tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                            if conflicts == 0 {
                                format!("Merged {actor} into feature {feature}")
                            } else {
                                format!(
                                    "Merged {actor} into {feature} with {conflicts} conflict file(s)"
                                )
                            }
                        }
                        Err(err) => format!("Merge failed: {err}"),
                    }
                }
                _ => "Merge cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Undo the last edit on the active file-workspace actor partition.
    pub(super) fn undo_file_workspace_edit(&mut self) {
        self.file_workspace_history("/api/v1/file-checkpoint/undo", "Undo");
    }

    /// Redo the last undone edit on the active file-workspace actor partition.
    pub(super) fn redo_file_workspace_edit(&mut self) {
        self.file_workspace_history("/api/v1/file-checkpoint/redo", "Redo");
    }

    /// Shared undo/redo driver: `base` is the endpoint prefix before the
    /// `{actor}` segment.
    fn file_workspace_history(&mut self, base: &'static str, label: &'static str) {
        let Some(actor) = self.file_workspace_actor() else {
            self.app
                .notice
                .set(format!("No file workspace to {label}."));
            return;
        };
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let path = format!("{base}/{}", crate::fetch::common::encode_segment(&actor));
            let outcome = match client
                .post_json::<_, serde_json::Value>(&path, &serde_json::json!({}))
                .await
            {
                Ok(snapshot) => {
                    let _ = tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
                    let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                    let snapshot = snapshot.as_str().unwrap_or("");
                    format!("{label} recorded ({})", short_id(snapshot))
                }
                Err(err) => format!("{label} failed: {err}"),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Undo the last input boundary of the selected checkpoint row's
    /// execution after confirmation. Preview runs first so an empty touch
    /// set reports "nothing to undo" without writing; otherwise the apply
    /// result reports the restored count. History refs are untouched.
    pub(super) fn undo_last_input_for_selected(&mut self) {
        let Some(row) = self.selected_checkpoint_row() else {
            self.app
                .notice
                .set("No checkpoint row selected.".to_string());
            return;
        };
        let execution = row.entity.clone();
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Undo last input",
            format!(
                "Undo the last input of {}? Preview runs first; files outside the touch set are never written.",
                short_id(&execution)
            ),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let preview: Result<serde_json::Value, String> = client
                        .post_json(
                            "/api/v1/file-checkpoint/undo/preview",
                            &serde_json::json!({ "execution_id": execution }),
                        )
                        .await
                        .map_err(|err| err.to_string());
                    match preview {
                        Err(err) => format!("Undo preview failed: {err}"),
                        Ok(value) => {
                            let files = value
                                .get("files")
                                .and_then(|v| v.as_array())
                                .map(|files| files.len())
                                .unwrap_or(0);
                            if files == 0 {
                                "Nothing to undo for the last input.".to_string()
                            } else {
                                match client
                                    .post_json::<_, serde_json::Value>(
                                        "/api/v1/file-checkpoint/undo/apply",
                                        &serde_json::json!({ "execution_id": execution }),
                                    )
                                    .await
                                {
                                    Ok(applied) => {
                                        let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                                        let _ =
                                            tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
                                        let _ = tx.send(Feedback::Refresh(ScreenKind::Dashboard));
                                        let restored = applied
                                            .get("restored")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0);
                                        format!(
                                            "Undone last input of {} ({restored} file(s))",
                                            short_id(&execution)
                                        )
                                    }
                                    Err(err) => format!("Undo failed: {err}"),
                                }
                            }
                        }
                    }
                }
                _ => "Undo cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Run a manual file-checkpoint store GC sweep after confirmation.
    pub(super) fn run_file_workspace_gc(&mut self) {
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Run GC",
            "Reclaim unreferenced file-checkpoint objects now?".to_string(),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    match client
                        .post_json::<_, serde_json::Value>(
                            "/api/v1/file-checkpoint/gc?keep_recent_heads=0",
                            &serde_json::json!({}),
                        )
                        .await
                    {
                        Ok(value) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Dashboard));
                            let removed = value
                                .get("removed_checkpoints")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            format!("GC completed (removed {removed} commits)")
                        }
                        Err(err) => format!("GC failed: {err}"),
                    }
                }
                _ => "GC cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Rebuild the file-checkpoint source index from the commit graph.
    pub(super) fn rebuild_file_workspace_index(&mut self) {
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            match client
                .post_json::<_, serde_json::Value>(
                    "/api/v1/file-checkpoint/index/rebuild",
                    &serde_json::json!({}),
                )
                .await
            {
                Ok(value) => {
                    let indexed = value
                        .get("indexed_commits")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let _ = tx.send(Feedback::Notice(format!(
                        "Source index rebuilt ({indexed} commits)"
                    )));
                }
                Err(err) => {
                    let _ = tx.send(Feedback::Notice(format!("Index rebuild failed: {err}")));
                }
            }
        }));
    }

    /// Begin a file edit session with an optional prompt label.
    pub(super) fn begin_edit_session(&mut self) {
        let rx = self
            .modals
            .push_with_result(Box::new(crate::prompt_modal::PromptModal::new(
                "Begin edit session",
                "Session label",
            )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Value(label)) => {
                    match client
                        .post_json::<_, serde_json::Value>(
                            "/api/v1/file-checkpoint/sessions",
                            &serde_json::json!({ "label": label }),
                        )
                        .await
                    {
                        Ok(value) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::EditSessions));
                            let id = value.as_str().unwrap_or("");
                            format!("Started edit session {label} ({})", short_id(id))
                        }
                        Err(err) => format!("Begin session failed: {err}"),
                    }
                }
                _ => "Begin session cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Roll back the selected edit session on the active execution's actor
    /// partition after confirmation.
    pub(super) fn rollback_selected_edit_session(&mut self) {
        let selected = self.app.screen.navigation.selected();
        let session = match self.display_data() {
            ScreenData::EditSessions(rows) => {
                let idx = selected.min(rows.len().saturating_sub(1));
                rows.get(idx).map(|row| row.id.clone())
            }
            _ => None,
        };
        let Some(session) = session else {
            self.app.notice.set("No edit session selected.".to_string());
            return;
        };
        let Some(actor) = self
            .sessions
            .active()
            .and_then(|session| session.session_id())
            .map(|id| format!("agent:{id}"))
        else {
            self.app
                .notice
                .set("No active execution to roll back.".to_string());
            return;
        };
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Roll back edit session",
            format!("Roll back session {} on {actor}?", short_id(&session)),
        )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let path = format!(
                        "/api/v1/file-checkpoint/sessions/{}/rollback/{}",
                        crate::fetch::common::encode_segment(&session),
                        crate::fetch::common::encode_segment(&actor)
                    );
                    match client
                        .post_json::<_, serde_json::Value>(&path, &serde_json::json!({}))
                        .await
                    {
                        Ok(_) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::EditSessions));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
                            format!("Rolled back session {}", short_id(&session))
                        }
                        Err(err) => format!("Rollback failed: {err}"),
                    }
                }
                _ => "Rollback cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Merge the named features into main. The feature list is prompted as
    /// whitespace/comma separated names; main merges are gated behind the
    /// prompt itself. Conflicted features abort the sequence and are reported.
    pub(super) fn merge_file_workspace_to_main(&mut self) {
        let Some(actor) = self.file_workspace_actor() else {
            self.app
                .notice
                .set("No file workspace to merge.".to_string());
            return;
        };
        let rx = self
            .modals
            .push_with_result(Box::new(crate::prompt_modal::PromptModal::new(
                "Merge to main",
                "Feature names (space or comma separated)",
            )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Value(text)) => {
                    let features: Vec<String> = text
                        .split(|c: char| c == ',' || c.is_whitespace())
                        .map(|name| name.trim().to_string())
                        .filter(|name| !name.is_empty())
                        .collect();
                    if features.is_empty() {
                        "Merge to main cancelled: no features".to_string()
                    } else {
                        match client
                            .post_json::<_, serde_json::Value>(
                                "/api/v1/file-checkpoint/merge-to-main",
                                &serde_json::json!({ "actor": actor, "features": features }),
                            )
                            .await
                        {
                            Ok(value) => {
                                let merge = value.get("merge_result");
                                let commit = merge
                                    .and_then(|m| m.get("commit_id"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let conflicts = merge
                                    .and_then(|m| m.get("conflict_files"))
                                    .and_then(|v| v.as_array())
                                    .map(|files| files.len())
                                    .unwrap_or(0);
                                let checkpoint = value
                                    .get("checkpoint_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let _ = tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
                                let _ = tx.send(Feedback::Refresh(ScreenKind::Conflicts));
                                let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                                let _ = tx.send(Feedback::Refresh(ScreenKind::Dashboard));
                                if conflicts == 0 {
                                    format!("Merged into main ({})", short_id(commit))
                                } else {
                                    format!(
                                        "Merged into main with {conflicts} conflict file(s); checkpoint {}",
                                        short_id(checkpoint)
                                    )
                                }
                            }
                            Err(err) => format!("Merge to main failed: {err}"),
                        }
                    }
                }
                _ => "Merge to main cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Append a revert commit for one merged feature head. The head commit id
    /// is prompted; other merged features are untouched.
    pub(super) fn rollback_file_workspace_feature(&mut self) {
        let Some(actor) = self.file_workspace_actor() else {
            self.app
                .notice
                .set("No file workspace to roll back.".to_string());
            return;
        };
        let rx = self
            .modals
            .push_with_result(Box::new(crate::prompt_modal::PromptModal::new(
                "Rollback feature",
                "Feature head commit id",
            )));
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Value(head)) => {
                    match client
                        .post_json::<_, serde_json::Value>(
                            "/api/v1/file-checkpoint/rollback-feature",
                            &serde_json::json!({ "actor": actor, "feature_head": head }),
                        )
                        .await
                    {
                        Ok(value) => {
                            let revert = value
                                .get("revert_commit")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let _ = tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Conflicts));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Checkpoints));
                            format!("Feature rolled back ({})", short_id(revert))
                        }
                        Err(err) => format!("Feature rollback failed: {err}"),
                    }
                }
                _ => "Feature rollback cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Currently selected conflict row, or `None` off the conflicts screen.
    fn selected_conflict_row(&self) -> Option<ConflictRow> {
        let ScreenData::Conflicts(rows) = self.display_data() else {
            return None;
        };
        let idx = self
            .app
            .screen
            .navigation
            .selected()
            .min(rows.len().saturating_sub(1));
        rows.get(idx).cloned()
    }

    /// Resolve the selected conflict file by accepting one whole side. Binary
    /// files are refused; main-line conflicts prompt for the feature name.
    pub(super) fn resolve_selected_conflict_side(&mut self, accept_ours: bool) {
        let Some(row) = self.selected_conflict_row() else {
            self.app.notice.set("No conflict selected.".to_string());
            return;
        };
        if row.binary {
            self.app.notice.set(
                "Binary conflicts cannot be resolved in the TUI; use the CLI or web app."
                    .to_string(),
            );
            return;
        }
        match row.partition.strip_prefix("refs/wf/feat/") {
            Some(feature) => {
                let feature = feature.to_string();
                self.spawn_conflict_resolution(row, feature, accept_ours);
            }
            None => {
                let rx =
                    self.modals
                        .push_with_result(Box::new(crate::prompt_modal::PromptModal::new(
                            "Resolve on main",
                            "Feature name",
                        )));
                let tx = self.feedback_tx.clone();
                let client = self.client.clone();
                self.tasks.push(tokio::spawn(async move {
                    let feature = match rx.await {
                        Ok(ModalResult::Value(value)) if !value.trim().is_empty() => value,
                        _ => {
                            let _ = tx.send(Feedback::Notice("Resolve cancelled".to_string()));
                            return;
                        }
                    };
                    let outcome = resolve_conflict_side(&client, &row, &feature, accept_ours).await;
                    let _ = tx.send(Feedback::Refresh(ScreenKind::Conflicts));
                    let _ = tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
                    let _ = tx.send(Feedback::Notice(outcome));
                }));
            }
        }
    }

    /// Spawn the side-choice resolution for a feature-ref conflict.
    fn spawn_conflict_resolution(&mut self, row: ConflictRow, feature: String, accept_ours: bool) {
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = resolve_conflict_side(&client, &row, &feature, accept_ours).await;
            let _ = tx.send(Feedback::Refresh(ScreenKind::Conflicts));
            let _ = tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }
}

/// Read the conflicted bytes at the merge commit, rebuild the whole file
/// with one side per region, and submit the resolution. Returns the user
/// notice text; never panics on a missing payload.
async fn resolve_conflict_side(
    client: &WfClient,
    row: &ConflictRow,
    feature: &str,
    accept_ours: bool,
) -> String {
    let actor = "manual";
    let content_path = format!(
        "/api/v1/file-checkpoint/content?actor={}&path={}&version={}",
        crate::fetch::common::encode_segment(actor),
        crate::fetch::common::encode_segment(&row.path),
        crate::fetch::common::encode_segment(&row.snapshot_id),
    );
    let content: serde_json::Value = match client.get_json(&content_path).await {
        Ok(value) => value,
        Err(err) => return format!("Conflict read failed: {err}"),
    };
    if content
        .get("is_binary")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return "Binary conflict: resolve it in the CLI or web app.".to_string();
    }
    if content
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return "Conflicted file is truncated (over 1 MiB); resolve it in the CLI or web app."
            .to_string();
    }
    let Some(text) = content.get("content").and_then(|v| v.as_str()) else {
        return "Conflicted content is empty; resolve it in the CLI or web app.".to_string();
    };
    let Some(resolved) = rebuild_conflict_content(text, accept_ours) else {
        return "No conflict markers found in the merged file; nothing to resolve.".to_string();
    };
    let body = serde_json::json!({
        "actor": actor,
        "feature": feature,
        "resolutions": { row.path.clone(): resolved },
    });
    match client
        .post_json::<_, serde_json::Value>("/api/v1/file-checkpoint/conflicts/resolve", &body)
        .await
    {
        Ok(value) => {
            let remaining = value.get("remaining").and_then(|v| v.as_u64()).unwrap_or(0);
            if remaining == 0 {
                format!("Resolved {}; no conflicts remain", row.path)
            } else {
                format!(
                    "Resolved {}; {remaining} file(s) still conflicted",
                    row.path
                )
            }
        }
        Err(err) => format!("Resolution failed: {err}"),
    }
}

/// Rebuild a whole-file resolution from diff3 markers, keeping one side per
/// region and every non-region line verbatim. Returns `None` when the bytes
/// carry no markers (already resolved or binary text).
fn rebuild_conflict_content(content: &str, accept_ours: bool) -> Option<String> {
    // 0 = normal text, 1 = ours, 2 = theirs, 3 = base (diff3).
    let mut state = 0u8;
    let mut saw_marker = false;
    let mut out: Vec<&str> = Vec::new();
    for line in content.lines() {
        match (state, line) {
            (0, "<<<<<<< ours") => {
                state = 1;
                saw_marker = true;
            }
            (1, l) if l.starts_with("|||||||") => state = 3,
            (1, "=======") => state = 2,
            (3, "=======") => state = 2,
            (2, l) if l.starts_with(">>>>>>>") => state = 0,
            (1, l) => {
                if accept_ours {
                    out.push(l);
                }
            }
            (3, _) => {}
            (2, l) => {
                if !accept_ours {
                    out.push(l);
                }
            }
            _ => out.push(line),
        }
    }
    if !saw_marker || state != 0 {
        return None;
    }
    let mut resolved = out.join("\n");
    if content.ends_with('\n') {
        resolved.push('\n');
    }
    Some(resolved)
}

async fn toggle_execution(
    client: &WfClient,
    id: &str,
    paused: bool,
) -> Result<(), wf_client::ClientError> {
    let action = if paused { "resume" } else { "pause" };
    if client
        .post_json::<_, serde_json::Value>(
            &format!("/api/v1/executions/{id}/{action}"),
            &serde_json::json!({}),
        )
        .await
        .is_ok()
    {
        return Ok(());
    }
    client
        .post_json::<_, serde_json::Value>(
            &format!("/api/v1/agent-loops/{id}/{action}"),
            &serde_json::json!({}),
        )
        .await
        .map(|_| ())
}

async fn cancel_execution(client: &WfClient, id: &str) -> Result<(), wf_client::ClientError> {
    if client
        .post_json::<_, serde_json::Value>(
            &format!("/api/v1/executions/{id}/cancel"),
            &serde_json::json!({}),
        )
        .await
        .is_ok()
    {
        return Ok(());
    }
    client
        .post_json::<_, serde_json::Value>(
            &format!("/api/v1/agent-loops/{id}/cancel"),
            &serde_json::json!({}),
        )
        .await
        .map(|_| ())
}

/// Parsed baseline selector for a sync rebase: every field is optional and
/// serializes as null when absent, mirroring the CLI flags.
struct RebaseParams {
    branch: Option<String>,
    commit_prefix: Option<String>,
    behind: Option<u64>,
    mode: String,
}

impl RebaseParams {
    fn describe(&self) -> String {
        format!(
            "branch={} prefix={} behind={} mode={}",
            self.branch.as_deref().unwrap_or("-"),
            self.commit_prefix.as_deref().unwrap_or("-"),
            self.behind
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string()),
            self.mode
        )
    }
}

/// Parse the single-line rebase selector the prompt collects: a bare token
/// is the branch, `prefix=` / `behind=` / `mode=` set the named fields.
/// Commit prefix and behind stay mutually exclusive, like the server rule.
fn parse_rebase_selector(selector: &str) -> Result<RebaseParams, String> {
    let mut branch: Option<String> = None;
    let mut commit_prefix: Option<String> = None;
    let mut behind: Option<u64> = None;
    let mut mode = "borrow".to_string();
    for token in selector.split_whitespace() {
        if let Some(prefix) = token.strip_prefix("prefix=") {
            if prefix.is_empty() {
                return Err("Rebase prefix is empty.".to_string());
            }
            commit_prefix = Some(prefix.to_string());
        } else if let Some(n) = token.strip_prefix("behind=") {
            let parsed: u64 = n
                .parse()
                .map_err(|_| format!("Rebase behind is not a number: {n}"))?;
            behind = Some(parsed);
        } else if let Some(name) = token.strip_prefix("mode=") {
            if name != "borrow" && name != "copy" {
                return Err(format!("Rebase mode must be borrow or copy: {name}"));
            }
            mode = name.to_string();
        } else {
            if branch.is_some() {
                return Err(format!("Rebase takes one branch: {token}"));
            }
            branch = Some(token.to_string());
        }
    }
    if commit_prefix.is_some() && behind.is_some() {
        return Err("Commit prefix and behind are mutually exclusive.".to_string());
    }
    Ok(RebaseParams {
        branch,
        commit_prefix,
        behind,
        mode,
    })
}

/// Map a sync write failure onto a user notice: conflicts ask for a later
/// retry without auto-retrying, missing state points at the file-checkpoint
/// state, and validation failures keep the server reason verbatim.
fn sync_failure_notice(label: &str, err: &wf_client::ClientError) -> String {
    let text = err.to_string();
    if text.contains("not found") || text.contains("NotFound") {
        format!("{label}: not found — check the file-checkpoint state ({text})")
    } else if text.contains("CONFLICT")
        || text.contains("Conflict")
        || text.contains("conflict")
        || text.contains("already running")
    {
        format!("{label}: conflict — retry later ({text})")
    } else {
        format!("{label} failed: {text}")
    }
}

/// Step `current` up or down the `steps` ladder, clamping at the ends.
fn step_option(current: usize, steps: &[usize], up: bool) -> usize {
    if up {
        steps
            .iter()
            .find(|step| **step > current)
            .copied()
            .unwrap_or(current)
    } else {
        steps
            .iter()
            .rev()
            .find(|step| **step < current)
            .copied()
            .unwrap_or(current)
    }
}

#[cfg(test)]
mod tests {
    use super::rebuild_conflict_content;
    use super::*;

    #[test]
    fn rebase_selector_parses_named_fields() {
        let params = parse_rebase_selector("main prefix=abc mode=copy").expect("parses");
        assert_eq!(params.branch.as_deref(), Some("main"));
        assert_eq!(params.commit_prefix.as_deref(), Some("abc"));
        assert_eq!(params.behind, None);
        assert_eq!(params.mode, "copy");
        let params = parse_rebase_selector("behind=3").expect("parses");
        assert_eq!(params.branch, None);
        assert_eq!(params.behind, Some(3));
        assert_eq!(params.mode, "borrow");
    }

    #[test]
    fn rebase_selector_rejects_bad_input() {
        assert!(parse_rebase_selector("a b").is_err());
        assert!(parse_rebase_selector("prefix=abc behind=3").is_err());
        assert!(parse_rebase_selector("mode=fast").is_err());
        assert!(parse_rebase_selector("behind=many").is_err());
    }

    #[test]
    fn rebuild_accepts_ours_over_theirs() {
        let merged = "head\n<<<<<<< ours\nours line\n=======\ntheirs line\n>>>>>>> theirs\ntail\n";
        assert_eq!(
            rebuild_conflict_content(merged, true).as_deref(),
            Some("head\nours line\ntail\n")
        );
        assert_eq!(
            rebuild_conflict_content(merged, false).as_deref(),
            Some("head\ntheirs line\ntail\n")
        );
    }

    #[test]
    fn rebuild_handles_diff3_base_section() {
        let merged = "<<<<<<< ours\na\n||||||| base\nb\n=======\nc\n>>>>>>> theirs\n";
        assert_eq!(
            rebuild_conflict_content(merged, true).as_deref(),
            Some("a\n")
        );
        assert_eq!(
            rebuild_conflict_content(merged, false).as_deref(),
            Some("c\n")
        );
    }

    #[test]
    fn rebuild_rejects_markerless_and_unterminated_content() {
        assert_eq!(rebuild_conflict_content("clean\n", true), None);
        assert_eq!(rebuild_conflict_content("<<<<<<< ours\na\n", true), None);
    }

    #[test]
    fn rebuild_preserves_body_without_trailing_newline() {
        let merged = "x\n<<<<<<< ours\nours\n=======\ntheirs\n>>>>>>> theirs";
        assert_eq!(
            rebuild_conflict_content(merged, true).as_deref(),
            Some("x\nours")
        );
    }
}
