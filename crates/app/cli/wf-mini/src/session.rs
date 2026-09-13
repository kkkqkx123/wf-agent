//! Native terminal session: line-discipline REPL over an append-only
//! terminal. stdout carries assistant text verbatim, stderr carries tool
//! lifecycle, summaries and errors, stdin reads one line per prompt. No raw
//! mode, no fullscreen redraw, no markdown re-rendering.

use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;

use wf_api::infra::stream::ExecutionStreamEvent;
use wf_api::ToolApprovalHandler;

use wf_cli_shared::domain::DomainHandle;
use wf_cli_shared::error::{CliError, CliResult};
use wf_cli_shared::turn::{stream_agent_turn, TurnKind, TurnParams};

use crate::approval::NativeApprovalHandler;
use crate::input::{self, History, SlashCommand};
use crate::output::{self, AppendWriter};

const PROMPT: &str = "> ";

/// Interactive session owning the domain handle until shutdown.
///
/// Each turn is independent: no conversation is carried between turns and
/// the prompt history is display-only. Workflows run through the headless
/// `run --workflow` form and the management subcommands, not here.
pub struct NativeSession {
    domain: DomainHandle,
    agent: Option<String>,
    model: Option<String>,
    auto_approve: bool,
    session_requested: bool,
    history: History,
}

impl NativeSession {
    /// Bootstrap the domain in interactive mode and snapshot the CLI-level
    /// agent and model overrides for every turn.
    pub async fn new(cli: &wf_cli_shared::args::Cli) -> CliResult<Self> {
        let domain = DomainHandle::from_cli(cli, wf_cli_shared::mode::CliMode::Tui).await?;
        Ok(Self {
            domain,
            agent: cli.agent.clone(),
            model: cli.model.clone(),
            auto_approve: cli
                .approval
                .as_deref()
                .is_some_and(|mode| mode.eq_ignore_ascii_case("auto")),
            session_requested: cli.session.is_some() || cli.resume,
            history: History::new(),
        })
    }

    /// Run the prompt loop until EOF or a quit command.
    pub async fn run(mut self) -> CliResult<()> {
        output::diag_line(
            "native mini — type a prompt, /help for commands, Ctrl-D to quit; each turn is independent (no memory between turns)",
        );
        self.warn_session_flags_ignored();
        loop {
            let line = tokio::select! {
                biased;
                _ = tokio::signal::ctrl_c() => {
                    output::diag_line("^C");
                    continue;
                }
                result = input::read_prompt_line(PROMPT) => result
                    .map_err(|e| CliError::Io(std::io::Error::other(format!("read prompt: {e}"))))?,
            };
            let Some(raw) = line else { break };
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(command) = input::parse_command(&trimmed) {
                if self.handle_command(command) {
                    break;
                }
                continue;
            }
            self.history.push(&trimmed);
            output::diag_line(&format!("user: {trimmed}"));
            self.run_turn(trimmed).await;
        }
        self.domain.shutdown().await?;
        Ok(())
    }

    /// Returns true when the session should exit.
    fn handle_command(&self, command: SlashCommand) -> bool {
        match command {
            SlashCommand::Quit => true,
            SlashCommand::Help => {
                output::diag_line("commands: /help show this help, /clear print a separator, /quit exit (Ctrl-D also exits)");
                output::diag_line("keys: Ctrl-C cancels the running turn, Ctrl-D quits");
                output::diag_line(
                    "notes: each turn is independent (no memory between turns); workflows run via `wf run --workflow` and the management subcommands",
                );
                false
            }
            SlashCommand::Clear => {
                output::diag_line("---");
                false
            }
        }
    }

    /// The native form keeps no session state across turns, so the resume
    /// flags cannot apply here. Warn once instead of ignoring them
    /// silently; validation already required sqlite storage for them.
    fn warn_session_flags_ignored(&self) {
        if self.session_requested {
            output::diag_line(
                "note: --session/--resume is ignored here; mini keeps no memory between turns",
            );
        }
    }

    async fn run_turn(&self, prompt: String) {
        let params = TurnParams {
            agent: self.agent.clone(),
            model: self.model.clone(),
            approve_prefixes: Vec::new(),
            kind: TurnKind::Agent { prompt },
        };
        match &self.domain {
            DomainHandle::Embedded(adapter) => {
                let handler: Arc<dyn ToolApprovalHandler> =
                    Arc::new(NativeApprovalHandler::new(Vec::new(), self.auto_approve));
                match stream_agent_turn(adapter.api_context(), &params, Some(handler)).await {
                    Ok((execution_id, stream)) => {
                        self.pump_embedded(&execution_id, stream).await;
                    }
                    Err(err) => output::diag_line(&format!("failed to start turn: {err}")),
                }
            }
            DomainHandle::Remote(remote) => {
                // The remote stream carries no execution id, so mint a local
                // correlation id for this turn's summary line. It identifies
                // the local turn, not a server-side execution.
                let correlation_id = wf_common::generate_id();
                match remote.client().stream_agent_execution(params).await {
                    Ok(stream) => self.pump_remote(&correlation_id, stream).await,
                    Err(err) => output::diag_line(&format!("failed to start turn: {err}")),
                }
            }
        }
    }

    async fn pump_embedded(
        &self,
        execution_id: &str,
        mut stream: wf_api::infra::stream::ExecutionEventStream,
    ) {
        let started = Instant::now();
        let mut renderer = TurnRenderer::new();
        let interrupted = loop {
            tokio::select! {
                biased;
                _ = tokio::signal::ctrl_c() => break true,
                event = stream.next() => {
                    let Some(event) = event else {
                        output::diag_line("agent stream ended without a terminal event");
                        break false;
                    };
                    if renderer.on_event(&event) {
                        break false;
                    }
                }
            }
        };
        drop(stream);
        renderer.finish();
        self.finish_turn(execution_id, started, interrupted);
    }

    async fn pump_remote(
        &self,
        correlation_id: &str,
        mut stream: impl futures::Stream<Item = Result<ExecutionStreamEvent, wf_cli_shared::remote::RemoteError>>
            + Unpin,
    ) {
        let started = Instant::now();
        let mut renderer = TurnRenderer::new();
        let (mut interrupted, mut failed) = (false, false);
        loop {
            tokio::select! {
                biased;
                _ = tokio::signal::ctrl_c() => {
                    interrupted = true;
                    break;
                }
                event = stream.next() => match event {
                    Some(Ok(event)) => {
                        if renderer.on_event(&event) {
                            break;
                        }
                    }
                    Some(Err(err)) => {
                        output::diag_line(&format!("remote stream error: {err}"));
                        failed = true;
                        break;
                    }
                    None => {
                        output::diag_line("remote stream ended without a terminal event");
                        failed = true;
                        break;
                    }
                },
            }
        }
        renderer.finish();
        if failed {
            return;
        }
        self.finish_turn(&format!("remote:{correlation_id}"), started, interrupted);
    }

    fn finish_turn(&self, execution_id: &str, started: Instant, interrupted: bool) {
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

/// Shorten an execution id for the summary line. Remote turns carry a
/// local `remote:`-prefixed correlation id (not a server execution id),
/// so the prefix is preserved while the random suffix is truncated.
fn short_execution_id(execution_id: &str) -> String {
    match execution_id.strip_prefix("remote:") {
        Some(rest) => format!("remote:{}", rest.chars().take(8).collect::<String>()),
        None => execution_id.chars().take(8).collect::<String>(),
    }
}

/// Renders one turn: assistant deltas append to stdout, everything else
/// goes to stderr. Returns true on a terminal event.
struct TurnRenderer {
    append: AppendWriter,
    line_pending: bool,
}

impl TurnRenderer {
    fn new() -> Self {
        Self {
            append: AppendWriter::new(),
            line_pending: false,
        }
    }

    fn on_event(&mut self, event: &ExecutionStreamEvent) -> bool {
        match event {
            ExecutionStreamEvent::Engine(_) => false,
            ExecutionStreamEvent::IterationStart { .. }
            | ExecutionStreamEvent::IterationEnd { .. } => {
                self.flush_stdout();
                false
            }
            ExecutionStreamEvent::LlmDelta { content } => {
                let ready = self.append.push(content);
                if !ready.is_empty() {
                    let _ = output::write_stdout(&ready);
                    self.line_pending = !ready.ends_with('\n');
                }
                false
            }
            ExecutionStreamEvent::ToolStart { tool_name, .. } => {
                self.end_stdout_line();
                output::diag_line(&format!("tool start: {tool_name}"));
                false
            }
            ExecutionStreamEvent::ToolEnd {
                tool_name, success, ..
            } => {
                self.end_stdout_line();
                if *success {
                    output::diag_line(&format!("tool ok: {tool_name}"));
                } else {
                    output::diag_line(&format!("tool failed: {tool_name}"));
                }
                false
            }
            ExecutionStreamEvent::ReasoningDelta { content } => {
                self.end_stdout_line();
                let _ = output::write_stderr(content);
                false
            }
            ExecutionStreamEvent::Usage {
                prompt_tokens,
                completion_tokens,
                cost,
            } => {
                self.end_stdout_line();
                match cost {
                    Some(value) => output::diag_line(&format!(
                        "usage: {prompt_tokens} prompt + {completion_tokens} completion tokens (~${value:.4})"
                    )),
                    None => output::diag_line(&format!(
                        "usage: {prompt_tokens} prompt + {completion_tokens} completion tokens"
                    )),
                }
                false
            }
            ExecutionStreamEvent::SubAgentStarted { name, .. } => {
                self.end_stdout_line();
                output::diag_line(&format!("subagent started: {name}"));
                false
            }
            ExecutionStreamEvent::SubAgentEnded { name, success, .. } => {
                self.end_stdout_line();
                if *success {
                    output::diag_line(&format!("subagent done: {name}"));
                } else {
                    output::diag_line(&format!("subagent failed: {name}"));
                }
                false
            }
            ExecutionStreamEvent::Completed { iterations, .. } => {
                self.flush_stdout();
                output::diag_line(&format!("completed in {iterations} iterations"));
                true
            }
            ExecutionStreamEvent::Failed { error } => {
                self.flush_stdout();
                output::diag_line(&format!("failed: {error}"));
                true
            }
            ExecutionStreamEvent::Interrupted { reason } => {
                self.flush_stdout();
                output::diag_line(&format!("interrupted: {reason}"));
                true
            }
        }
    }

    fn flush_stdout(&mut self) {
        let remaining = self.append.take_remaining();
        if !remaining.is_empty() {
            let _ = output::write_stdout(&remaining);
            self.line_pending = !remaining.ends_with('\n');
        }
        self.end_stdout_line();
    }

    fn end_stdout_line(&mut self) {
        if self.line_pending {
            let _ = output::write_stdout("\n");
            self.line_pending = false;
        }
    }

    fn finish(&mut self) {
        self.flush_stdout();
        let _ = std::io::Write::flush(&mut std::io::stderr());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_embedded_execution_id() {
        assert_eq!(short_execution_id("abcdef123456"), "abcdef12");
    }

    #[test]
    fn preserves_remote_correlation_prefix() {
        assert_eq!(short_execution_id("remote:abcdef123456"), "remote:abcdef12");
    }
}
