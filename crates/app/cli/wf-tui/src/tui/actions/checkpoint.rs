//! Checkpoint actions: detail, chain, restore and delete.

use crate::modal::{ConfirmModal, ModalResult, ModelPicker};
use crate::overlay::Feedback;
use crate::screens::{short_id, ApprovalRow, CheckpointRow, ConflictRow, ScreenData, ScreenKind};
use wf_client::WfClient;

use super::*;
use crate::tui::data_cache::DataResult;
use crate::tui::TuiApp;

impl TuiApp {
    /// Currently selected checkpoint row, or `None` off the screen.
    pub(crate) fn selected_checkpoint_row(&self) -> Option<CheckpointRow> {
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
    pub(crate) fn open_selected_checkpoint_detail(&mut self) {
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
    pub(crate) fn open_selected_checkpoint_chain(&mut self) {
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
    pub(crate) fn restore_selected_checkpoint(&mut self) {
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
    pub(crate) fn delete_selected_checkpoint(&mut self) {
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
    pub(crate) fn open_selected_cached_detail(&mut self) {
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
    pub(crate) fn open_selected_file_timeline(&mut self) {
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
}
