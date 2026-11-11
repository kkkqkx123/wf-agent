//! File sync actions: export, import and rebase.

use crate::modal::{ConfirmModal, ModalResult, ModelPicker};
use crate::overlay::Feedback;
use crate::screens::{short_id, ApprovalRow, CheckpointRow, ConflictRow, ScreenData, ScreenKind};
use wf_client::WfClient;

use super::conflict::{parse_rebase_selector, sync_failure_notice};
use super::*;
use crate::tui::data_cache::DataResult;
use crate::tui::TuiApp;

impl TuiApp {
    /// Open the git-sync screen from the Files screen. The screen fetches
    /// its three read-only sections on mount; no file selection is needed.
    pub(crate) fn open_file_sync(&mut self) {
        self.invalidate(ScreenKind::FileSync);
        self.goto(ScreenKind::FileSync);
    }

    /// Effective auto-commit default behind the sync screen, for the export
    /// request body. Off the sync screen there is nothing to export.
    pub(crate) fn file_sync_auto_commit(&self) -> Option<bool> {
        match self.display_data() {
            ScreenData::FileSync(data) => Some(data.auto_commit),
            _ => None,
        }
    }

    /// Export the main tree into the user workspace after confirmation. The
    /// confirmation is the overwrite guard; auto-commit rides the cached
    /// sync default and the message is server-suggested.
    pub(crate) fn export_file_sync(&mut self) {
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
    pub(crate) fn import_file_sync(&mut self) {
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
    pub(crate) fn rebase_file_sync(&mut self) {
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
    pub(crate) fn confirm_file_sync_rebase(&mut self, selector: &str) {
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
}
