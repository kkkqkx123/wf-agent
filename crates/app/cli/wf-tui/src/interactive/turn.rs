//! Turn lifecycle and streaming settlement for the interactive controller:
//! starting agent turns, pumping server events, and feeding the markdown
//! stream / pacer into the scrollback.

use futures::StreamExt;

use wf_types::events::ExecutionStreamEvent;

use crate::reducer::Phase;
use crate::stream_pacer::PacerOp;
use crate::tool_output::DetailLevel;
use crate::transcript::{HistoryLine, LineState, Role};
use crate::turn::{TurnKind, TurnParams};

use super::{InteractiveController, InteractiveEvent, NestedAgentLoopTarget};

/// Truncate a prompt into a short session title.
fn truncate_title(prompt: &str) -> String {
    truncate_title_for_title(prompt)
}

/// Truncate helper shared with the title request path so the LLM fallback
/// matches the plain truncation mode.
pub(crate) fn truncate_title_for_title(prompt: &str) -> String {
    let cleaned: String = prompt
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        return String::new();
    }
    const LIMIT: usize = 40;
    if cleaned.chars().count() <= LIMIT {
        cleaned
    } else {
        let mut out: String = cleaned.chars().take(LIMIT.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

impl InteractiveController {
    /// Resolved auto title mode, defaulting to truncation.
    pub(crate) fn auto_title_mode() -> wf_cli_shared::app_config::AutoTitleMode {
        use wf_cli_shared::app_config::AutoTitleMode;
        let Some(path) = crate::app_config::config_file_path() else {
            return AutoTitleMode::Truncate;
        };
        match crate::app_config::ConfigManager::load(&path) {
            Ok(manager) => manager.config().terminal.auto_title_mode(),
            Err(_) => AutoTitleMode::Truncate,
        }
    }
}

/// Adapt server SSE frames into execution stream events, skipping the
/// stream metadata frame and unparsable frames.
pub(crate) fn cmd_stream_frames(
    stream: impl futures::Stream<Item = Result<wf_client::SseFrame, wf_client::ClientError>>
        + Send
        + 'static,
) -> impl futures::Stream<Item = Result<ExecutionStreamEvent, wf_client::ClientError>> + Unpin {
    use futures::StreamExt;
    Box::pin(stream)
        .filter_map(|item| async move {
            match item {
                Err(e) => Some(Err(e)),
                Ok(frame) => match wf_client::sse::decode_execution_frame(&frame) {
                    wf_client::sse::ExecutionFrame::Metadata(_) => None,
                    wf_client::sse::ExecutionFrame::Engine(base) => {
                        Some(Ok(ExecutionStreamEvent::Engine(base)))
                    }
                    wf_client::sse::ExecutionFrame::Event(event) => Some(Ok(event)),
                    wf_client::sse::ExecutionFrame::Raw(_) => None,
                },
            }
        })
        .boxed()
}

impl InteractiveController {
    /// Submit a prompt: start a turn, or append it to the running input
    /// surface while a turn is active. One turn runs at a time; queued
    /// prompts drain in order once the active turn reaches a terminal event.
    /// While a nested workflow execution is attached, submits address the
    /// selected nested target by execution id plus node id and never fall
    /// back to a bare loop id.
    pub(crate) fn submit_prompt(&mut self, prompt: String) {
        if self.turn_task.is_some() {
            if self.nested_append_enabled() {
                let mode = Some(self.append_mode.clone());
                self.submit_nested_text(None, prompt, mode);
                return;
            }
            if let Some(loop_id) = self.active_agent_loop_id.clone() {
                let client = self.client.clone();
                let tx = self.tx.clone();
                let mode = self.append_mode.clone();
                let text = prompt.clone();
                let fallback = prompt.clone();
                tokio::spawn(async move {
                    let result = client
                        .append_agent_loop(&loop_id, text, Some(mode), None)
                        .await;
                    let notice = match result {
                        Ok(_) => "input appended to the running agent".to_string(),
                        Err(e) => {
                            let _ = tx.send(InteractiveEvent::QueuePrompt(fallback));
                            format!("append failed: {e}; kept in local queue (not server buffered), retry from /queued")
                        }
                    };
                    let _ = tx.send(InteractiveEvent::Notice(notice));
                });
                self.footer.show_notice("appending to the running agent...");
                return;
            }
            // No loop id is ready yet: stage the input in the local queue
            // without disguising it as server buffered.
            self.prompt_queue.push(prompt);
            self.footer
                .show_notice("loop not ready, staged locally (not server buffered)");
            return;
        }
        let model = self.model_override_name();
        self.start_turn(prompt, None, model);
    }

    /// The model name the session override resolves to. The panel stores the
    /// profile id; the turn request carries the model name.
    pub(crate) fn model_override_name(&self) -> Option<String> {
        let id = self.model_override.as_deref()?;
        self.panel_profiles
            .iter()
            .find(|profile| profile.id == id)
            .map(|profile| profile.model.clone())
            .or_else(|| Some(id.to_string()))
    }
}

/// List nested targets in the background. Loud listings report rows via
/// the loaded event; silent recalibrations only update state. Loud
/// failures surface as a failed event; silent ones only warn in logs.
fn spawn_nested_list(
    client: wf_client::WfClient,
    tx: tokio::sync::mpsc::UnboundedSender<InteractiveEvent>,
    execution: String,
    silent: bool,
) {
    tokio::spawn(async move {
        match client.list_workflow_agent_loops(&execution).await {
            Ok(loops) => {
                let targets = loops
                    .into_iter()
                    .map(|entry| {
                        let mut target = NestedAgentLoopTarget::from(entry);
                        target.execution_id = execution.clone();
                        target
                    })
                    .collect();
                let _ = tx.send(InteractiveEvent::NestedTargetsLoaded {
                    execution_id: execution,
                    targets,
                    silent,
                });
            }
            Err(e) => {
                if silent {
                    tracing::warn!(target: "wf_cli", error = %e, execution_id = %execution, "nested target recalibration failed");
                } else {
                    let _ = tx.send(InteractiveEvent::NestedTargetsFailed {
                        execution_id: execution,
                        error: e.to_string(),
                    });
                }
            }
        }
    });
}

/// Re-list nested targets after a successful nested command so the pending
/// counts stay calibrated. Best-effort: list failures stay silent here.
pub(crate) fn recalibrate_nested_targets(
    client: &wf_client::WfClient,
    tx: &tokio::sync::mpsc::UnboundedSender<InteractiveEvent>,
    execution: &str,
) {
    spawn_nested_list(client.clone(), tx.clone(), execution.to_string(), true);
}

/// One session-memory message carrying plain text in the given role.
fn session_message(role: wf_types::message::MessageRole, text: &str) -> wf_types::message::Message {
    wf_types::message::Message {
        id: wf_common::generate_id(),
        role,
        content: wf_types::message::MessageContentValue::Text(text.to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    }
}

/// User prompts carried by replay lines, oldest first: accent-role rows
/// starting with the live (`❯ `) or replay (`> `) prompt mark. Used to seed
/// the composer's input history so resumed sessions recall earlier prompts.
fn user_prompts_from_lines(lines: &[HistoryLine]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| line.role == Role::Accent)
        .filter_map(|line| {
            let text = line.raw_lines(1000).join(" ");
            let stripped = text
                .strip_prefix("> ")
                .or_else(|| text.strip_prefix(">"))
                .or_else(|| text.strip_prefix("❯ "))
                .or_else(|| text.strip_prefix("❯"))?;
            let trimmed = stripped.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

mod nested;
mod queue;
mod stream;
