//! Native terminal session: line-discipline REPL over an append-only
//! terminal. stdout carries assistant text verbatim, stderr carries tool
//! lifecycle, summaries and errors, stdin reads one line per prompt. No raw
//! mode, no fullscreen redraw, no markdown re-rendering.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;

use wf_api::infra::stream::ExecutionStreamEvent;
use wf_api::ToolApprovalHandler;

use wf_cli_shared::domain::DomainHandle;
use wf_cli_shared::error::{CliError, CliResult};
use wf_cli_shared::turn::{stream_agent_turn, TurnKind, TurnParams};

use crate::approval::NativeApprovalHandler;
use crate::input::{self, History, LineReader, SlashCommand};
use crate::output::{self};
use crate::transcript::Transcript;

mod turn_renderer;

use turn_renderer::{short_execution_id, FinishedTurn, RealSink, TurnRenderer};

const PROMPT: &str = "> ";

/// Window in which a second prompt-time Ctrl-C quits the session.
const PROMPT_QUIT_WINDOW: Duration = Duration::from_secs(5);

/// Tracks prompt-time Ctrl-C presses: one press abandons the current line
/// and stays, a second press inside the window quits. Any consumed line
/// resets the armer, so only back-to-back presses quit.
#[derive(Debug, Default)]
struct QuitArmer {
    last: Option<Instant>,
}

impl QuitArmer {
    fn note_press(&mut self, now: Instant) -> bool {
        let quit = self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) <= PROMPT_QUIT_WINDOW);
        self.last = Some(now);
        quit
    }

    fn reset(&mut self) {
        self.last = None;
    }
}

/// Interactive session owning the domain handle until shutdown.
///
/// Turns share one transcript: every completed round seeds the next run, so
/// the model remembers earlier rounds. Workflows run through the headless
/// `run --workflow` form and the management subcommands, not here.
pub struct NativeSession {
    domain: DomainHandle,
    agent: Option<String>,
    model: Option<String>,
    auto_approve: bool,
    approval_llm: bool,
    history: History,
    lines: Arc<tokio::sync::Mutex<LineReader>>,
    transcript: Transcript,
    anchor: String,
    /// Turn-cancel flag: the approval handler sets it when Ctrl-C is
    /// pressed during a prompt; the pump loop watches it and treats the
    /// turn as interrupted.
    cancel_tx: tokio::sync::watch::Sender<bool>,
    cancel_rx: tokio::sync::watch::Receiver<bool>,
}

/// What a slash command asks the loop to do.
enum CommandAction {
    Continue,
    Exit,
    /// Run this prompt as a question (used by `/rerun`).
    Ask(String),
}

impl NativeSession {
    /// Run one prompt as a question: record, stream, memorize, drain.
    async fn ask(&mut self, question: String) {
        self.history.push(&question);
        output::diag_line(&format!("user: {question}"));
        if let Some(finished) = self.run_turn(question.clone()).await {
            self.record_completed_turn(&question, &finished).await;
        }
        self.discard_stale_lines().await;
    }
    /// Bootstrap the domain in interactive mode and snapshot the CLI-level
    /// agent and model overrides for every turn. Resolves the session anchor
    /// (`--session`, `--resume`, or fresh) and restores its transcript.
    pub async fn new(cli: &wf_cli_shared::args::Cli) -> CliResult<Self> {
        let domain = DomainHandle::from_cli(cli, wf_cli_shared::mode::CliMode::Tui).await?;
        let (anchor, transcript) = Self::restore_session(&domain, cli).await;
        let history = match input::resolve_history_path(cli.history_file.as_deref(), cli.no_history)
        {
            Some(path) => History::load(path),
            None => History::new(),
        };
        Ok(Self {
            domain,
            agent: cli.agent.clone(),
            model: cli.model.clone(),
            auto_approve: cli
                .approval
                .as_deref()
                .is_some_and(|mode| mode.eq_ignore_ascii_case("auto")),
            approval_llm: cli
                .approval
                .as_deref()
                .is_some_and(|mode| mode.eq_ignore_ascii_case("llm")),
            history,
            lines: Arc::new(tokio::sync::Mutex::new(LineReader::for_stdin())),
            transcript,
            anchor,
            cancel_tx: tokio::sync::watch::channel(false).0,
            cancel_rx: tokio::sync::watch::channel(false).1,
        })
    }

    /// Run the prompt loop until EOF or a quit command.
    pub async fn run(mut self) -> CliResult<()> {
        output::diag_line(
            "native mini — type a prompt, /help for commands, Ctrl-D to quit; remembers the full history",
        );
        output::diag_line(&format!(
            "session {}; resume later with --session {} --storage sqlite:<path>",
            self.anchor, self.anchor
        ));
        if !self.transcript.is_empty() {
            output::diag_line(&format!(
                "restored {} messages into memory",
                self.transcript.len()
            ));
        }
        let mut quitter = QuitArmer::default();
        loop {
            let lines = Arc::clone(&self.lines);
            let line = tokio::select! {
                biased;
                _ = tokio::signal::ctrl_c() => {
                    // The terminal already discarded the half-typed line;
                    // drop any queued whole lines through the shared drain,
                    // then stay (first press) or quit (second press).
                    self.discard_stale_lines().await;
                    if quitter.note_press(Instant::now()) {
                        break;
                    }
                    output::diag_line("press Ctrl-C again to quit");
                    continue;
                }
                result = async { lines.lock().await.read_prompt_line(PROMPT).await } => result
                    .map_err(|e| CliError::Io(std::io::Error::other(format!("read prompt: {e}"))))?,
            };
            let Some(raw) = line else { break };
            quitter.reset();
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(command) = input::parse_command(&trimmed) {
                match self.handle_command(command) {
                    CommandAction::Exit => break,
                    CommandAction::Continue => continue,
                    CommandAction::Ask(question) => self.ask(question).await,
                }
                continue;
            }
            self.ask(trimmed).await;
        }
        self.domain.shutdown().await?;
        Ok(())
    }

    /// Drop lines that arrived while the prompt was not listening (late
    /// approval answers, lines typed mid-stream) so they are never misread
    /// as the next question. Leaves one trace on stderr when anything was
    /// dropped.
    async fn discard_stale_lines(&self) {
        let dropped = self.lines.lock().await.drain();
        if dropped > 0 {
            output::diag_line(&format!(
                "cleared {dropped} stale input line{} (typed while busy)",
                if dropped == 1 { "" } else { "s" }
            ));
        }
    }
    /// Returns what the loop should do next.
    fn handle_command(&mut self, command: SlashCommand) -> CommandAction {
        match command {
            SlashCommand::Quit => CommandAction::Exit,
            SlashCommand::Help => {
                output::diag_line("commands: /help show this help, /clear print a separator, /new start a fresh conversation, /quit exit (Ctrl-D also exits)");
                output::diag_line("history: /history list recent prompts, /rerun [n] re-ask prompt n (default: last)");
                output::diag_line("keys: Ctrl-C cancels the running turn, Ctrl-C twice at the prompt quits, Ctrl-D quits");
                output::diag_line(
                    "notes: the full history stays in memory; prompt history is plain text on disk (see --history-file/--no-history); workflows run via `wf run --workflow` and the management subcommands",
                );
                CommandAction::Continue
            }
            SlashCommand::Clear => {
                output::diag_line("---");
                CommandAction::Continue
            }
            SlashCommand::New => {
                self.transcript.clear();
                self.anchor = format!("mini-{}", wf_common::generate_id());
                output::diag_line(&format!(
                    "started a new conversation (session {})",
                    self.anchor
                ));
                CommandAction::Continue
            }
            SlashCommand::History => {
                if self.history.is_empty() {
                    output::diag_line("no history yet");
                } else {
                    for (number, entry) in self.history.recent(20) {
                        output::diag_line(&format!("{number}: {entry}"));
                    }
                }
                CommandAction::Continue
            }
            SlashCommand::Rerun(number) => {
                let resolved = number.unwrap_or_else(|| self.history.len());
                match self.history.get(resolved) {
                    Some(entry) => {
                        let question = entry.to_string();
                        output::diag_line(&format!("rerun {resolved}: {question}"));
                        CommandAction::Ask(question)
                    }
                    None => {
                        output::diag_line(&format!(
                            "no history entry {resolved} (history holds {} entries)",
                            self.history.len()
                        ));
                        CommandAction::Continue
                    }
                }
            }
        }
    }

    /// Resolve the session anchor and its transcript: an explicit
    /// `--session` id wins, `--resume` picks the most recent stored
    /// session, otherwise a fresh anchor starts empty. Restore problems
    /// fall back to an empty transcript without failing startup.
    async fn restore_session(
        domain: &DomainHandle,
        cli: &wf_cli_shared::args::Cli,
    ) -> (String, Transcript) {
        let mut transcript = Transcript::new();
        if let Some(id) = cli.session.clone() {
            let stored = Self::load_history(domain, &id).await;
            let restored = transcript.restore(stored);
            if restored > 0 {
                output::diag_line(&format!("resumed session {id} ({restored} messages)"));
            } else {
                output::diag_line(&format!(
                    "session '{id}' has no saved messages; starting fresh"
                ));
            }
            return (id, transcript);
        }
        if cli.resume {
            match Self::load_latest(domain).await {
                Some((id, stored)) => {
                    let restored = transcript.restore(stored);
                    if restored > 0 {
                        output::diag_line(&format!("resumed session {id} ({restored} messages)"));
                        return (id, transcript);
                    }
                    output::diag_line("latest session has no saved messages; starting fresh");
                }
                None => {
                    output::diag_line("no previous session found; starting fresh");
                }
            }
        }
        (format!("mini-{}", wf_common::generate_id()), transcript)
    }

    /// Stored messages of one session, oldest first. Empty on any failure.
    async fn load_history(domain: &DomainHandle, id: &str) -> Vec<wf_types::message::Message> {
        match domain {
            DomainHandle::Embedded(adapter) => {
                wf_api::agent::agent_message::conversation_history(adapter.api_context(), id, None)
                    .await
                    .unwrap_or_default()
            }
            DomainHandle::Remote(remote) => remote.client().get_conversation(id).await,
        }
    }

    /// Most recently active stored session and its messages. Embedded mode
    /// asks the storage layer for the session anchor with the newest stored
    /// message (mini persists every turn under its anchor); remote mode picks
    /// the newest loop summary. Returns `None` when nothing resumable exists.
    async fn load_latest(
        domain: &DomainHandle,
    ) -> Option<(String, Vec<wf_types::message::Message>)> {
        match domain {
            DomainHandle::Embedded(adapter) => {
                let anchor = wf_api::entity::message::latest_session_anchor(adapter.api_context())
                    .await
                    .ok()
                    .flatten()?;
                let stored = Self::load_history(domain, &anchor).await;
                if stored.is_empty() {
                    return None;
                }
                Some((anchor, stored))
            }
            DomainHandle::Remote(remote) => {
                let mut newest: Option<(i64, String)> = None;
                for summary in remote.client().list_loop_summaries().await {
                    let id = summary
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let start = summary
                        .get("start_time")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    if id.is_empty() {
                        continue;
                    }
                    if newest.as_ref().is_none_or(|(ts, _)| start > *ts) {
                        newest = Some((start, id));
                    }
                }
                let (_, id) = newest?;
                let stored = remote.client().get_conversation(&id).await;
                if stored.is_empty() {
                    return None;
                }
                Some((id, stored))
            }
        }
    }

    /// File a completed turn into the transcript and persist it under the
    /// session anchor so `--session`/`--resume` can restore it later.
    /// Interrupted and failed turns never enter memory.
    async fn record_completed_turn(&mut self, question: &str, finished: &FinishedTurn) {
        if !finished.outcome.completed {
            return;
        }
        let created = self
            .transcript
            .push_pair(question, &finished.outcome.assistant_text);
        self.persist_turn(&finished.execution_id, &created).await;
    }

    /// Best-effort per-turn persist under the session anchor. Embedded mode
    /// only (remote has no message write surface); failures warn and never
    /// break the session.
    async fn persist_turn(&self, execution_id: &str, messages: &[wf_types::message::Message]) {
        let DomainHandle::Embedded(adapter) = &self.domain else {
            return;
        };
        for message in messages {
            if let Err(err) = wf_api::entity::message::add_message(
                adapter.api_context(),
                execution_id,
                Some(&self.anchor),
                message.clone(),
            )
            .await
            {
                output::diag_line(&format!("warning: session persist failed: {err}"));
                break;
            }
        }
    }
    /// Run one turn seeded with the session transcript. Returns `None`
    /// when the turn never started; the outcome tells whether the round
    /// counts as memory.
    async fn run_turn(&self, prompt: String) -> Option<FinishedTurn> {
        let params = TurnParams {
            agent: self.agent.clone(),
            model: self.model.clone(),
            approve_prefixes: Vec::new(),
            conversation: self.transcript.messages().to_vec(),
            kind: TurnKind::Agent { prompt },
        };
        match &self.domain {
            DomainHandle::Embedded(adapter) => {
                let handler: Arc<dyn ToolApprovalHandler> = if self.approval_llm {
                    // `--approval llm`: the shared fail-closed LLM reviewer
                    // answers the engine's Ask decisions.
                    Arc::new(wf_runtime::tool_approval::LlmApprovalHandler::new(
                        Arc::clone(&adapter.api_context_arc().llm_gateway),
                        self.model
                            .clone()
                            .unwrap_or_else(|| wf_api::DEFAULT_MODEL.to_string()),
                    ))
                } else {
                    Arc::new(NativeApprovalHandler::new(
                        Vec::new(),
                        self.auto_approve,
                        Arc::clone(&self.lines),
                        self.cancel_tx.clone(),
                    ))
                };
                let options = wf_runtime::tool_approval::headless_approval_options(Some(
                    adapter.api_context(),
                ));
                match stream_agent_turn(
                    adapter.api_context(),
                    &params,
                    Some(options),
                    Some(handler),
                )
                .await
                {
                    Ok((execution_id, stream)) => {
                        // The embedded stream yields plain events; normalize
                        // to the shared pump's Result shape.
                        let outcome = self.pump_stream(&execution_id, stream.map(Ok)).await;
                        Some(FinishedTurn {
                            execution_id,
                            outcome,
                        })
                    }
                    Err(err) => {
                        output::diag_line(&format!("failed to start turn: {err}"));
                        None
                    }
                }
            }
            DomainHandle::Remote(remote) => {
                // The remote stream carries no execution id, so mint a local
                // correlation id for this turn's summary line. It identifies
                // the local turn, not a server-side execution.
                let correlation_id = wf_common::generate_id();
                match remote.client().stream_agent_execution(params).await {
                    Ok(stream) => {
                        // Wrap transport errors as `Some(..)`; the shared
                        // pump uses `Err(None)` only for premature end.
                        let normalized = stream.map(|item| item.map_err(Some));
                        let outcome = self
                            .pump_stream(&format!("remote:{correlation_id}"), normalized)
                            .await;
                        Some(FinishedTurn {
                            execution_id: format!("remote:{correlation_id}"),
                            outcome,
                        })
                    }
                    Err(err) => {
                        output::diag_line(&format!("failed to start turn: {err}"));
                        None
                    }
                }
            }
        }
    }

    /// One pump loop shared by both domains. `Stream::Item` is normalized to
    /// `Result<event, Option<error>>`: `Err(None)` is a stream that ended
    /// without a terminal event, `Err(Some(..))` a transport failure. Both
    /// interrupt handling and terminal-event detection are written exactly
    /// once here.
    async fn pump_stream<S>(&self, execution_id: &str, mut stream: S) -> TurnOutcome
    where
        S: futures::Stream<
                Item = Result<ExecutionStreamEvent, Option<wf_cli_shared::remote::RemoteError>>,
            > + Unpin,
    {
        let started = Instant::now();
        let mut renderer = TurnRenderer::<RealSink>::new();
        // The cancel receiver needs `&mut` for poll; keep a local clone so
        // the pump can hold `&self` like every other session method.
        let mut cancel = self.cancel_rx.clone();
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
                event = stream.next() => match event {
                    Some(Ok(event)) => {
                        if let Some(completed) = renderer.on_event(&event) {
                            let text = renderer.assistant_text();
                            break TurnOutcome::finished(completed, text);
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
        self.finish_turn(execution_id, started, outcome.interrupted);
        outcome
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

/// What one turn produced: whether it counts as memory and the full
/// assistant text for the transcript.
struct TurnOutcome {
    completed: bool,
    interrupted: bool,
    assistant_text: String,
}

impl TurnOutcome {
    fn finished(completed: bool, assistant_text: String) -> Self {
        Self {
            completed,
            interrupted: false,
            assistant_text,
        }
    }

    fn interrupted(assistant_text: String) -> Self {
        Self {
            completed: false,
            interrupted: true,
            assistant_text,
        }
    }

    fn uncompleted(assistant_text: String) -> Self {
        Self {
            completed: false,
            interrupted: false,
            assistant_text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_armer_quits_only_on_back_to_back_presses() {
        let start = Instant::now();
        let mut armer = QuitArmer::default();
        assert!(!armer.note_press(start));
        assert!(armer.note_press(start + Duration::from_secs(1)));
    }

    #[test]
    fn quit_armer_ignores_stale_presses() {
        let start = Instant::now();
        let mut armer = QuitArmer::default();
        assert!(!armer.note_press(start));
        assert!(!armer.note_press(start + PROMPT_QUIT_WINDOW + Duration::from_secs(1)));
    }

    #[test]
    fn quit_armer_reset_disarms() {
        let start = Instant::now();
        let mut armer = QuitArmer::default();
        assert!(!armer.note_press(start));
        armer.reset();
        assert!(!armer.note_press(start + Duration::from_secs(1)));
    }
}
