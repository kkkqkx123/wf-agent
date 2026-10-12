//! Destructive / mutating actions dispatched from key routes: delete
//! workflow, pause/resume and cancel executions, default model selection.

use std::sync::Arc;

use crate::modal::{ConfirmModal, ModalResult, ModelPicker};
use crate::overlay::Feedback;
use crate::screens::{short_id, CheckpointRow, ScreenData, ScreenKind};

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
        let adapter = Arc::clone(&self.adapter);
        let tx = self.data_tx.clone();
        self.tasks.push(tokio::spawn(async move {
            let ctx = adapter.api_context();
            let result = wf_api::checkpoint::record::get_checkpoint(&ctx.storage, &task_id)
                .await
                .map(|checkpoint| crate::fetch::format_checkpoint_detail(&checkpoint))
                .map_err(Into::into);
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
        let adapter = Arc::clone(&self.adapter);
        let tx = self.data_tx.clone();
        self.tasks.push(tokio::spawn(async move {
            let ctx = adapter.api_context();
            let result = wf_api::checkpoint::record::chain_for_execution(ctx, &task_entity, None)
                .await
                .map(|chain| crate::fetch::format_checkpoint_chain(&chain))
                .map_err(Into::into);
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
        let adapter = Arc::clone(&self.adapter);
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let ctx = adapter.api_context();
                    let result = if is_loop {
                        wf_api::agent::agent_checkpoint::restore(ctx, &entity, &id)
                            .await
                            .map(|checkpoint| checkpoint.id)
                            .map_err(|err| err.to_string())
                    } else {
                        wf_api::workflow::workflow_execution::restore_checkpoint(ctx, &id)
                            .await
                            .map(|restored| restored.execution_id)
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
        let adapter = Arc::clone(&self.adapter);
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let ctx = adapter.api_context();
                    match wf_api::checkpoint::record::delete_checkpoint(&ctx.storage, &id).await {
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
        let adapter = Arc::clone(&self.adapter);
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let ctx = adapter.api_context();
                    match wf_api::workflow::definition::delete_workflow(ctx, &id).await {
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
        let adapter = Arc::clone(&self.adapter);
        self.tasks.push(tokio::spawn(async move {
            let ctx = adapter.api_context();
            let result = if paused {
                ctx.resume_execution(&id).await
            } else {
                ctx.pause_execution(&id).await
            };
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
        let adapter = Arc::clone(&self.adapter);
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let ctx = adapter.api_context();
                    match ctx.cancel_execution(&id).await {
                        Ok(()) => {
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Executions));
                            format!("Cancelled execution {id}")
                        }
                        Err(err) => format!("Cancel failed: {err}"),
                    }
                }
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
        let idx = self
            .app
            .screen
            .navigation
            .selected()
            .min(data.profiles.len().saturating_sub(1));
        let Some(_profile) = data.profiles.get(idx) else {
            return;
        };
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
        let adapter = Arc::clone(&self.adapter);
        self.tasks.push(tokio::spawn(async move {
            let result = match rx.await {
                Ok(ModalResult::Value(id)) => {
                    let ctx = adapter.api_context();
                    match wf_api::llm::llm_profile::set_default(ctx, &id).await {
                        Ok(()) => {
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
