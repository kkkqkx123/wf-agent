//! Destructive / mutating actions dispatched from key routes: delete
//! workflow, pause/resume and cancel executions, default model selection,
//! checkpoint detail/chain/restore/delete.

use crate::modal::{ConfirmModal, ModalResult, ModelPicker};
use crate::overlay::Feedback;
use crate::screens::{short_id, CheckpointRow, ScreenData, ScreenKind};
use wf_client::WfClient;

use super::data_cache::DataResult;
use super::TuiApp;

impl TuiApp {
    /// Currently selected checkpoint row, or `None` off the screen.
    fn selected_checkpoint_row(&self) -> Option<CheckpointRow> {
        let ScreenData::Checkpoints(rows) = self.current_data() else {
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

    /// Open the read-only chain viewer for the selected row's execution.
    pub(super) fn open_selected_checkpoint_chain(&mut self) {
        let Some(row) = self.selected_checkpoint_row() else {
            return;
        };
        let entity = row.entity.clone();
        let task_entity = entity.clone();
        let client = self.client.clone();
        let tx = self.data_tx.clone();
        self.tasks.push(tokio::spawn(async move {
            let result = client
                .get_json::<serde_json::Value>(&format!(
                    "/api/v1/executions/{task_entity}/checkpoints/chain"
                ))
                .await
                .map(|chain| crate::fetch::format_checkpoint_chain(&chain))
                .map_err(crate::error::CliError::from);
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

    /// Spawn a delete after the user confirms the currently selected workflow.
    pub(super) fn delete_selected_workflow(&mut self) {
        let rows = match self.current_data() {
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
        let rows = match self.current_data() {
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
        let rows = match self.current_data() {
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
