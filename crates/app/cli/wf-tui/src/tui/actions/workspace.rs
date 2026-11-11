//! File workspace actions: submit, merge, undo and sessions.

use crate::modal::{ConfirmModal, ModalResult, ModelPicker};
use crate::overlay::Feedback;
use crate::screens::{short_id, ApprovalRow, CheckpointRow, ConflictRow, ScreenData, ScreenKind};
use wf_client::WfClient;

use super::*;
use crate::tui::data_cache::DataResult;
use crate::tui::TuiApp;

impl TuiApp {
    /// Actor partition behind the file-workspace screen: the selected row's
    /// actor, or the active session's `agent:{id}` when the list is empty.
    pub(crate) fn file_workspace_actor(&self) -> Option<String> {
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
    pub(crate) fn submit_file_workspace_for_review(&mut self) {
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
    pub(crate) fn merge_file_workspace_to_feature(&mut self) {
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
    pub(crate) fn undo_file_workspace_edit(&mut self) {
        self.file_workspace_history("/api/v1/file-checkpoint/undo", "Undo");
    }

    /// Redo the last undone edit on the active file-workspace actor partition.
    pub(crate) fn redo_file_workspace_edit(&mut self) {
        self.file_workspace_history("/api/v1/file-checkpoint/redo", "Redo");
    }

    /// Shared undo/redo driver: `base` is the endpoint prefix before the
    /// `{actor}` segment.
    pub(crate) fn file_workspace_history(&mut self, base: &'static str, label: &'static str) {
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
    pub(crate) fn undo_last_input_for_selected(&mut self) {
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
    pub(crate) fn run_file_workspace_gc(&mut self) {
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
    pub(crate) fn rebuild_file_workspace_index(&mut self) {
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
    pub(crate) fn begin_edit_session(&mut self) {
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
    pub(crate) fn rollback_selected_edit_session(&mut self) {
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
    pub(crate) fn merge_file_workspace_to_main(&mut self) {
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
    pub(crate) fn rollback_file_workspace_feature(&mut self) {
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
}
