//! Mini session turn execution: run, pump and finish.

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
    /// Run one turn seeded with the session transcript. Returns `None`
    /// when the turn never started; the outcome tells whether the round
    /// counts as memory. Staged images ride the turn and are consumed by
    /// a started turn; a turn that never starts leaves them staged.
    pub(crate) async fn run_turn(&mut self, prompt: String) -> Option<FinishedTurn> {
        let params = TurnParams {
            agent: self.agent.clone(),
            model: self.model.clone(),
            approve_prefixes: Vec::new(),
            timeout_ms: self.timeout_ms,
            context: self.session_context.clone(),
            conversation: self.transcript.messages().to_vec(),
            kind: TurnKind::Agent {
                prompt,
                images: self.staged_images.clone(),
                attachments: self.staged_attachments.clone(),
            },
        };
        let body = params.agent_request_body();
        // The stream carries no execution id, so mint a local correlation
        // id for this turn's summary line. It identifies the local turn,
        // not a server-side execution.
        let correlation_id = wf_common::generate_id();
        let execution_id = format!("remote:{correlation_id}");
        match self
            .client
            .post_stream(&wf_client::endpoints::agent_cli_stream(), &body)
            .await
        {
            Ok(stream) => {
                self.title_frame = 0;
                self.write_terminal_title(Some(0));
                let stop = Arc::new(self.cancel_tx.subscribe());
                let approver = MiniApprover::new(
                    &self.client,
                    self.approval_timeout_secs,
                    Arc::clone(&self.lines),
                    self.cancel_tx.clone(),
                )
                .with_stderr_color(self.display.stderr_color);
                let approver_task = tokio::spawn(async move { approver.watch(stop).await });
                let normalized = map_frames(stream);
                let outcome = self.pump_stream(&execution_id, normalized).await;
                approver_task.abort();
                // A started turn consumes its staged images and attachments, win or lose.
                self.staged_images.clear();
                self.staged_attachments.clear();
                Some(FinishedTurn { outcome })
            }
            Err(err) => {
                output::diag_line(&format!("failed to start turn: {err}"));
                None
            }
        }
    }

    /// One pump loop shared by every turn. `Stream::Item` is normalized to
    /// `Result<event, Option<error>>`: `Err(None)` is a stream that ended
    /// without a terminal event, `Err(Some(..))` a transport failure. Both
    /// interrupt handling and terminal-event detection are written exactly
    /// once here.
    pub(crate) async fn pump_stream<S>(&mut self, execution_id: &str, mut stream: S) -> TurnOutcome
    where
        S: futures::Stream<Item = Result<ExecutionStreamEvent, Option<wf_client::ClientError>>>
            + Unpin,
    {
        let started = Instant::now();
        let mut renderer = TurnRenderer::<RealSink>::new(self.display);
        renderer.set_plugin_display(
            self.plugin_markdown_rules.clone(),
            self.plugin_tool_summaries
                .iter()
                .map(|summary| tui_core::tool_output::ToolSummaryTemplate {
                    tool_name: summary.tool_name.clone(),
                    call_template: summary.call_template.clone(),
                    result_template: summary.result_template.clone(),
                })
                .collect(),
        );
        // The cancel receiver needs `&mut` for poll; keep a local clone so
        // the pump can hold `&self` like every other session method.
        let mut cancel = self.cancel_rx.clone();
        let mut heartbeat = tokio::time::Instant::now() + TITLE_HEARTBEAT;
        let outcome = loop {
            tokio::select! {
                biased;
                _ = tokio::signal::ctrl_c() => {
                    break TurnOutcome::interrupted(renderer.assistant_text());
                }
                changed = cancel.changed() => {
                    if changed.is_err() || !*cancel.borrow_and_update() {
                        continue;
                    }
                    break TurnOutcome::interrupted(renderer.assistant_text());
                }
                _ = tokio::time::sleep_until(heartbeat) => {
                    // Heartbeat tick: flip the title frame so a frozen title
                    // signals a stalled process even with no stream traffic.
                    self.title_frame = self.title_frame.wrapping_add(1);
                    self.write_terminal_title(Some(self.title_frame));
                    heartbeat = tokio::time::Instant::now() + TITLE_HEARTBEAT;
                    continue;
                }
                event = stream.next() => match event {
                    Some(Ok(event)) => {
                        if let ExecutionStreamEvent::Engine(base) = &event {
                            if let Some(id) = base.agent_loop_id.clone() {
                                self.current_loop_id = Some(id);
                            }
                        }
                        let iterations = match &event {
                            ExecutionStreamEvent::Completed { iterations, .. } => {
                                Some(*iterations)
                            }
                            _ => None,
                        };
                        if let Some(completed) = renderer.on_event(&event) {
                            let text = renderer.assistant_text();
                            break TurnOutcome::finished(completed, text, iterations);
                        }
                    }
                    Some(Err(Some(err))) => {
                        output::diag_line(&format!("stream error: {err}"));
                        break TurnOutcome::uncompleted(renderer.assistant_text());
                    }
                    Some(Err(None)) => {
                        output::diag_line("stream ended without a terminal event");
                        break TurnOutcome::uncompleted(renderer.assistant_text());
                    }
                    None => {
                        output::diag_line("stream ended without a terminal event");
                        break TurnOutcome::uncompleted(renderer.assistant_text());
                    }
                },
            }
        };
        renderer.finish();
        // Back to the plain session title regardless of how the turn ended.
        self.write_terminal_title(None);
        self.finish_turn(execution_id, started, outcome.interrupted);
        outcome
    }

    pub(crate) fn finish_turn(&self, execution_id: &str, started: Instant, interrupted: bool) {
        if interrupted {
            output::diag_line("interrupted");
            return;
        }
        let short = short_execution_id(execution_id);
        output::diag_line(&format!(
            "done exec:{short} in {}ms",
            started.elapsed().as_millis()
        ));
    }
}
