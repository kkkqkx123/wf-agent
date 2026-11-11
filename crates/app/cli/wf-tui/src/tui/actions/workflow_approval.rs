//! Workflow and approval actions: executions and tool decisions.

use crate::modal::{ConfirmModal, ModalResult, ModelPicker};
use crate::overlay::Feedback;
use crate::screens::{short_id, ApprovalRow, CheckpointRow, ConflictRow, ScreenData, ScreenKind};
use wf_client::WfClient;

use super::conflict::{cancel_execution, toggle_execution};
use super::*;
use crate::tui::data_cache::DataResult;
use crate::tui::TuiApp;

impl TuiApp {
    /// Spawn a delete after the user confirms the currently selected workflow.
    pub(crate) fn delete_selected_workflow(&mut self) {
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
    pub(crate) fn toggle_selected_execution(&mut self) {
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
    pub(crate) fn cancel_selected_execution(&mut self) {
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
    pub(crate) fn pick_default_model(&mut self) {
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
    pub(crate) fn selected_approval_row(&self) -> Option<ApprovalRow> {
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
    pub(crate) fn approve_selected_approval(&mut self) {
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
    pub(crate) fn reject_selected_approval(&mut self) {
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
    pub(crate) fn create_checkpoint_for_selected(&mut self) {
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
    pub(crate) fn delete_checkpoints_for_selected_entity(&mut self) {
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
}
