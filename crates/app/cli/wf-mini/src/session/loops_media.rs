//! Mini session nested loops and media staging.

use super::turn_renderer::{short_execution_id, FinishedTurn, RealSink, TurnRenderer};
use super::*;
use crate::approval::MiniApprover;
use crate::display::DisplayConfig;
use crate::editor::{terminal_cols, EditorSnapshot, PromptEvent};
use crate::input::{self, AttachArgs, History, ImageArgs, LineReader, PluginSlash, SlashCommand};
use crate::output::{self};
use crate::suggest::{PluginCompletionSource, SuggestBroker, SuggestEngine};
use crate::transcript::Transcript;
use futures::StreamExt;
use std::io::IsTerminal;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wf_cli_shared::display::{parse_toggle, MarkdownMode};
use wf_cli_shared::error::{CliError, CliResult};
use wf_cli_shared::turn::{TurnKind, TurnParams};
use wf_client::{WfClient, WorkflowNestedAgentLoop};
use wf_types::events::ExecutionStreamEvent;

impl NativeSession {
    /// Submit steering/follow-up text to a nested workflow node when a
    /// node is given or an execution is attached; otherwise fall back to
    /// the current loop-only path. Failures only diag and keep the text.
    pub(crate) async fn submit_loop_text(
        &mut self,
        node: Option<String>,
        text: String,
        mode: &str,
    ) {
        if node.is_some() || self.nested_execution_id.is_some() {
            let Some(execution) = self.nested_execution_id.clone() else {
                self.keep_failed_input(&text);
                output::diag_line(
                    "no workflow execution attached; run /loops <execution> first (input kept in history)",
                );
                return;
            };
            let target = match self.resolve_nested_node(node) {
                Ok(target) => target,
                Err(reason) => {
                    self.keep_failed_input(&text);
                    output::diag_line(&format!("{reason} (input kept in history)"));
                    return;
                }
            };
            match self
                .client
                .append_workflow_nested_agent_loop(
                    &execution,
                    &target,
                    text.clone(),
                    Some(mode.to_string()),
                    Some("cli".to_string()),
                )
                .await
            {
                Ok(_) => {
                    self.nested_selected_node = Some(target.clone());
                    output::diag_line(&format!("{mode} input appended to {target}"));
                }
                Err(err) => {
                    self.keep_failed_input(&text);
                    output::diag_line(&format!(
                        "append to {target} failed: {err} (input kept in history)"
                    ));
                }
            }
            return;
        }
        if let Some(id) = self.current_loop_id.clone() {
            match self
                .client
                .append_agent_loop(&id, text.clone(), Some(mode.to_string()), None)
                .await
            {
                Ok(_) => output::diag_line(if mode == "steering" {
                    "steering input appended"
                } else {
                    "follow-up input appended"
                }),
                Err(err) => {
                    self.keep_failed_input(&text);
                    output::diag_line(&format!("append failed: {err} (input kept in history)"));
                }
            }
        } else {
            self.keep_failed_input(&text);
            output::diag_line("no active agent loop to append to (input kept in history)");
        }
    }

    /// Switch the default append mode of a nested node or, without nested
    /// context, of the current loop.
    pub(crate) async fn set_loop_append_mode(&mut self, node: Option<String>, mode: String) {
        if node.is_some() || self.nested_execution_id.is_some() {
            let Some(execution) = self.nested_execution_id.clone() else {
                output::diag_line("no workflow execution attached; run /loops <execution> first");
                return;
            };
            let target = match self.resolve_nested_node(node) {
                Ok(target) => target,
                Err(reason) => {
                    output::diag_line(&reason);
                    return;
                }
            };
            match self
                .client
                .set_workflow_nested_append_mode(&execution, &target, &mode)
                .await
            {
                Ok(_) => {
                    self.nested_selected_node = Some(target.clone());
                    output::diag_line(&format!("append mode of {target} set to {mode}"));
                }
                Err(err) => output::diag_line(&format!("set mode failed: {err}")),
            }
            return;
        }
        if let Some(id) = self.current_loop_id.clone() {
            match self.client.set_agent_loop_append_mode(&id, &mode).await {
                Ok(_) => output::diag_line(&format!("append mode set to {mode}")),
                Err(err) => output::diag_line(&format!("set mode failed: {err}")),
            }
        } else {
            output::diag_line("no active agent loop");
        }
    }

    /// Convert pending follow-ups of a nested node or, without nested
    /// context, of the current loop.
    pub(crate) async fn convert_loop_pending(&mut self, node: Option<String>) {
        if node.is_some() || self.nested_execution_id.is_some() {
            let Some(execution) = self.nested_execution_id.clone() else {
                output::diag_line("no workflow execution attached; run /loops <execution> first");
                return;
            };
            let target = match self.resolve_nested_node(node) {
                Ok(target) => target,
                Err(reason) => {
                    output::diag_line(&reason);
                    return;
                }
            };
            match self
                .client
                .convert_workflow_nested_pending(&execution, &target)
                .await
            {
                Ok(_) => {
                    self.nested_selected_node = Some(target.clone());
                    output::diag_line(&format!(
                        "pending follow-ups of {target} converted to steering"
                    ));
                }
                Err(err) => output::diag_line(&format!("convert failed: {err}")),
            }
            return;
        }
        if let Some(id) = self.current_loop_id.clone() {
            match self.client.convert_pending_to_steering(&id).await {
                Ok(_) => output::diag_line("pending follow-ups converted to steering"),
                Err(err) => output::diag_line(&format!("convert failed: {err}")),
            }
        } else {
            output::diag_line("no active agent loop");
        }
    }

    /// Handle the `/image` staging command: list staged images, drop them,
    /// or load new paths or remote URLs onto the stage. Loading failures leave the
    /// existing stage untouched; the combined stage is validated so an
    /// oversized stage is reported at stage time, not at send time.
    /// Newly staged images inherit the session detail (`/image-detail`).
    pub(crate) fn stage_images(&mut self, args: ImageArgs) {
        match args {
            ImageArgs::List => {
                if self.staged_images.is_empty() {
                    output::diag_line(
                        "no staged images (usage: /image <path-or-url>... | /image rm <n>)",
                    );
                } else {
                    for (index, image) in self.staged_images.iter().enumerate() {
                        let line = format!(
                            "[{}] {} ({})",
                            index + 1,
                            image.media_type.as_deref().unwrap_or("image"),
                            wf_cli_shared::turn::describe_user_image(image)
                        );
                        output::diag_line(&crate::render::highlight_mentions(
                            &line,
                            self.display.stderr_color,
                        ));
                    }
                }
                output::diag_line(&format!(
                    "image detail for new stages: {}",
                    Self::detail_label(self.staged_image_detail)
                ));
                output::diag_line(&wf_cli_shared::turn::stage_budget_line(
                    &self.staged_images,
                    &self.staged_attachments,
                ));
            }
            ImageArgs::Clear => {
                self.staged_images.clear();
                output::diag_line("staged images cleared");
            }
            ImageArgs::Remove(index) => {
                if index == 0 || index > self.staged_images.len() {
                    output::diag_line(&format!(
                        "no staged image {index} (1..={})",
                        self.staged_images.len()
                    ));
                } else {
                    self.staged_images.remove(index - 1);
                    output::diag_line(&format!("removed staged image {index}"));
                }
            }
            ImageArgs::Stage(paths) => {
                match wf_cli_shared::turn::load_user_image_specs(&paths, self.staged_image_detail) {
                    Err(err) => output::diag_line(&format!("cannot stage images: {err}")),
                    Ok(loaded) => {
                        let mut combined = self.staged_images.clone();
                        combined.extend(loaded);
                        if let Err(reason) = wf_types::message::validate_user_images(&combined) {
                            output::diag_line(&format!(
                                "cannot stage images: {reason} (stage unchanged)"
                            ));
                            return;
                        }
                        let added = combined.len() - self.staged_images.len();
                        self.staged_images = combined;
                        output::diag_line(&format!(
                            "staged {added} image{} ({} total, sent with the next turn)",
                            if added == 1 { "" } else { "s" },
                            self.staged_images.len()
                        ));
                        output::diag_line(&wf_cli_shared::turn::stage_budget_line(
                            &self.staged_images,
                            &self.staged_attachments,
                        ));
                    }
                }
            }
        }
    }

    /// Handle the `/attach` staging command: list staged text files, drop
    /// them, or load new paths onto the stage. Loading reuses the headless
    /// `--attach` reader so truncation markers and limits agree; failures
    /// leave the existing stage untouched.
    pub(crate) fn stage_attachments(&mut self, args: AttachArgs) {
        match args {
            AttachArgs::List => {
                if self.staged_attachments.is_empty() {
                    output::diag_line(
                        "no staged files (usage: /attach <path>... | /attach rm <n>)",
                    );
                } else {
                    for (index, attachment) in self.staged_attachments.iter().enumerate() {
                        let line = format!(
                            "[{}] {}",
                            index + 1,
                            wf_cli_shared::turn::describe_user_attachment(attachment)
                        );
                        output::diag_line(&crate::render::highlight_mentions(
                            &line,
                            self.display.stderr_color,
                        ));
                    }
                }
                output::diag_line(&wf_cli_shared::turn::stage_budget_line(
                    &self.staged_images,
                    &self.staged_attachments,
                ));
            }
            AttachArgs::Clear => {
                self.staged_attachments.clear();
                output::diag_line("staged files cleared");
            }
            AttachArgs::Remove(index) => {
                if index == 0 || index > self.staged_attachments.len() {
                    output::diag_line(&format!(
                        "no staged file {index} (1..={})",
                        self.staged_attachments.len()
                    ));
                } else {
                    self.staged_attachments.remove(index - 1);
                    output::diag_line(&format!("removed staged file {index}"));
                }
            }
            AttachArgs::Stage(paths) => {
                let specs: Vec<std::path::PathBuf> =
                    paths.iter().map(std::path::PathBuf::from).collect();
                match wf_cli_shared::turn::load_user_attachment_specs(&specs) {
                    Err(err) => output::diag_line(&format!("cannot stage files: {err}")),
                    Ok(loaded) => {
                        let mut combined = self.staged_attachments.clone();
                        combined.extend(loaded);
                        if let Err(reason) = wf_types::message::validate_user_attachments(&combined)
                        {
                            output::diag_line(&format!(
                                "cannot stage files: {reason} (stage unchanged)"
                            ));
                            return;
                        }
                        let added = combined.len() - self.staged_attachments.len();
                        self.staged_attachments = combined;
                        output::diag_line(&format!(
                            "staged {added} file{} ({} total, sent with the next turn)",
                            if added == 1 { "" } else { "s" },
                            self.staged_attachments.len()
                        ));
                        output::diag_line(&wf_cli_shared::turn::stage_budget_line(
                            &self.staged_images,
                            &self.staged_attachments,
                        ));
                    }
                }
            }
        }
    }

    pub(crate) fn detail_label(detail: Option<wf_types::message::ImageDetail>) -> &'static str {
        match detail {
            None => "provider default",
            Some(wf_types::message::ImageDetail::Auto) => "auto",
            Some(wf_types::message::ImageDetail::Low) => "low",
            Some(wf_types::message::ImageDetail::High) => "high",
            Some(wf_types::message::ImageDetail::Original) => "original",
        }
    }

    pub(crate) fn set_image_detail(&mut self, raw: Option<String>) {
        match raw.as_deref().map(str::trim) {
            None | Some("") => {
                output::diag_line(&format!("image detail: {}", Self::detail_label(self.staged_image_detail)));
            }
            Some("clear") | Some("default") => {
                self.staged_image_detail = None;
                output::diag_line("image detail cleared (provider default)");
            }
            Some(value) => match wf_cli_shared::turn::parse_image_detail_arg(Some(value)) {
                Ok(detail) => {
                    self.staged_image_detail = detail;
                    output::diag_line(&format!("image detail: {}", Self::detail_label(self.staged_image_detail)));
                }
                Err(err) => output::diag_line(&format!("invalid image detail: {err} (usage: /image-detail [auto|low|high|original|clear])")),
            },
        }
    }

    /// List the nested agent-loop targets of the attached execution (or
    /// attach to the given one first). Refreshes the snapshot and keeps
    /// the selection minimal: a single target is auto-selected, a stale
    /// selection is dropped, multiple targets need an explicit `--node`.
    pub(crate) async fn list_nested_loops(&mut self, execution: Option<String>) {
        if let Some(execution) = execution {
            self.nested_execution_id = Some(execution);
            self.nested_selected_node = None;
        }
        let Some(execution) = self.nested_execution_id.clone() else {
            output::diag_line("usage: /loops <execution-id>");
            return;
        };
        match self.client.list_workflow_agent_loops(&execution).await {
            Ok(loops) => {
                if loops.is_empty() {
                    self.nested_loops = Vec::new();
                    self.nested_selected_node = None;
                    output::diag_line(&format!("no nested agent loops for execution {execution}"));
                    return;
                }
                for entry in &loops {
                    let marker =
                        if self.nested_selected_node.as_deref() == Some(entry.node_id.as_str()) {
                            "*"
                        } else {
                            " "
                        };
                    output::diag_line(&format!(
                        "{marker} node {} status {} steering_pending {} followup_pending {} append_mode {} live {} loop {}",
                        entry.node_id,
                        entry.status,
                        entry.steering_pending,
                        entry.followup_pending,
                        entry.append_mode.as_deref().unwrap_or("-"),
                        entry.live,
                        entry.agent_loop_id,
                    ));
                }
                if loops.len() == 1 {
                    let only = loops[0].node_id.clone();
                    self.nested_selected_node = Some(only.clone());
                    output::diag_line(&format!("selected nested target {only}"));
                } else if let Some(selected) = self.nested_selected_node.clone() {
                    if !loops.iter().any(|entry| entry.node_id == selected) {
                        self.nested_selected_node = None;
                        output::diag_line(
                            "previous selection is gone; pass --node <id> to select a target",
                        );
                    }
                } else {
                    output::diag_line("multiple nested targets; pass --node <id> to select one");
                }
                self.nested_loops = loops;
            }
            Err(err) => output::diag_line(&format!("list loops failed: {err}")),
        }
    }
}
