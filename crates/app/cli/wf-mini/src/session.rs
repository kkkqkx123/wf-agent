//! Native terminal session: line-discipline REPL over an append-only
//! terminal. stdout carries assistant text (verbatim, plain or styled per
//! display config), stderr carries tool lifecycle, summaries and errors,
//! stdin reads one line per prompt. No fullscreen redraw: prompts run in a
//! single-row raw-mode editor with ghost-text suggestions when stdin and
//! stderr are TTYs, and fall back to the cooked line reader otherwise.
//!
//! Every turn runs against `wf-server`: the transcript seeds the request,
//! the server streams execution events back, and the transcript keeps the
//! assistant answers in memory. Session restore reads the server-side
//! conversation ledger (`--session` id or the newest loop for `--resume`).

use std::io::IsTerminal;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;

use wf_client::WfClient;
use wf_types::events::ExecutionStreamEvent;

use wf_cli_shared::display::{parse_toggle, MarkdownMode};
use wf_cli_shared::error::{CliError, CliResult};
use wf_cli_shared::turn::{TurnKind, TurnParams};

use crate::approval::MiniApprover;
use crate::display::DisplayConfig;
use crate::editor::{terminal_cols, EditorSnapshot, PromptEvent};
use crate::input::{self, History, LineReader, SlashCommand};
use crate::output::{self};
use crate::suggest::{SuggestBroker, SuggestEngine};
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

/// Interactive session over one server connection.
///
/// Turns share one transcript: every completed round seeds the next run, so
/// the model remembers earlier rounds. Workflows run through the headless
/// `run --workflow` form and the management subcommands, not here.
pub struct NativeSession {
    client: WfClient,
    timeout_ms: Option<u64>,
    agent: Option<String>,
    model: Option<String>,
    approval_timeout_secs: u64,
    display: DisplayConfig,
    history: History,
    lines: Arc<tokio::sync::Mutex<LineReader>>,
    transcript: Transcript,
    anchor: String,
    /// Raw-mode prompt editor is usable: stdin and stderr are TTYs.
    raw_prompt: bool,
    /// The previously submitted prompt (backs the `match_prev_cmd`
    /// suggestion strategy).
    prev_prompt: Option<String>,
    /// Suggestion pipeline shared by every prompt.
    suggest: Arc<SuggestEngine>,
    broker: Arc<SuggestBroker>,
    /// Turn-cancel flag: the approval prompter sets it when Ctrl-C is
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
        self.prev_prompt = Some(question.clone());
        output::diag_line(&format!("user: {question}"));
        if let Some(finished) = self.run_turn(question.clone()).await {
            self.record_completed_turn(&question, &finished);
        }
        self.discard_stale_lines().await;
    }
    /// Connect to the server and snapshot the CLI-level agent and model
    /// overrides for every turn. Resolves the session anchor (`--session`,
    /// `--resume`, or fresh) and restores its transcript from the
    /// server-side conversation ledger.
    pub async fn new(cli: &wf_cli_shared::args::Cli) -> CliResult<Self> {
        let handle = wf_cli_shared::ClientHandle::from_cli(cli).await?;
        let client = handle.into_client();
        let (anchor, transcript) = Self::restore_session(&client, cli).await;
        let history = match input::resolve_history_path(cli.history_file.as_deref(), cli.no_history)
        {
            Some(path) => History::load(path),
            None => History::new(),
        };
        let stdout_tty = std::io::stdout().is_terminal();
        let stderr_tty = std::io::stderr().is_terminal();
        let display = DisplayConfig::resolve(cli, stdout_tty, stderr_tty);
        let raw_prompt = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
        Ok(Self {
            client,
            timeout_ms: cli.timeout,
            agent: cli.agent.clone(),
            model: cli.model.clone(),
            approval_timeout_secs: display.approval_timeout_secs,
            display,
            history,
            lines: Arc::new(tokio::sync::Mutex::new(LineReader::for_stdin())),
            transcript,
            anchor,
            raw_prompt,
            prev_prompt: None,
            suggest: Arc::new(SuggestEngine::new()),
            broker: Arc::new(SuggestBroker::new()),
            cancel_tx: tokio::sync::watch::channel(false).0,
            cancel_rx: tokio::sync::watch::channel(false).1,
        })
    }

    /// Run the prompt loop until EOF or a quit command.
    pub async fn run(mut self) -> CliResult<()> {
        output::diag_line(
            "native mini — type a prompt, /help for commands, Ctrl-D to quit; remembers the full history",
        );
        output::diag_line(&format!("display: {}", self.display.describe()));
        output::diag_line(&format!(
            "session {}; resume later with --session {}",
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
            let event = if self.raw_prompt {
                self.read_prompt_raw().await?
            } else {
                self.read_prompt_cooked().await?
            };
            match event {
                PromptEvent::Eof => break,
                PromptEvent::Interrupt => {
                    // The terminal already discarded the half-typed line;
                    // drop any queued whole lines through the shared drain,
                    // then stay (first press) or quit (second press).
                    self.discard_stale_lines().await;
                    if quitter.note_press(Instant::now()) {
                        break;
                    }
                    output::diag_line("press Ctrl-C again to quit");
                }
                PromptEvent::Line(raw) => {
                    quitter.reset();
                    let trimmed = raw.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Some(command) = input::parse_command(&trimmed) {
                        match self.handle_command(command).await {
                            CommandAction::Exit => break,
                            CommandAction::Continue => continue,
                            CommandAction::Ask(question) => self.ask(question).await,
                        }
                        continue;
                    }
                    self.ask(trimmed).await;
                }
            }
        }
        Ok(())
    }

    /// One prompt through the raw-mode editor. The event stream is the only
    /// stdin reader on the TTY pipeline; it is borrowed for the prompt and
    /// handed back afterwards. Nothing else reads stdin while a prompt is
    /// active. Pipe sessions never reach here (raw needs dual TTY).
    async fn read_prompt_raw(&mut self) -> CliResult<PromptEvent> {
        if !self.lines.lock().await.is_tty() {
            return Ok(PromptEvent::Eof);
        }
        let snapshot = EditorSnapshot {
            history: self.history.snapshot(),
            prev_prompt: self.prev_prompt.clone(),
            prompt: PROMPT.to_string(),
            term_cols: terminal_cols(),
        };
        let engine = Arc::clone(&self.suggest);
        let broker = Arc::clone(&self.broker);
        let stream = self.lines.lock().await.take_event_stream();
        let Some(stream) = stream else {
            return Ok(PromptEvent::Eof);
        };
        let (event, stream) = crate::editor::run_tty(stream, snapshot, engine, broker).await;
        self.lines.lock().await.restore_event_stream(stream);
        Ok(event)
    }

    /// Cooked fallback (piped stdin or stderr): the plain line reader with
    /// a SIGINT select, matching the pre-editor behavior.
    async fn read_prompt_cooked(&mut self) -> CliResult<PromptEvent> {
        enum CookedRead {
            Line(String),
            Eof,
            Interrupt,
        }
        let lines = Arc::clone(&self.lines);
        let result = tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => CookedRead::Interrupt,
            read = async { lines.lock().await.read_prompt_line(PROMPT).await } => {
                match read {
                    Ok(Some(line)) => CookedRead::Line(line),
                    Ok(None) => CookedRead::Eof,
                    Err(e) => {
                        return Err(CliError::Io(std::io::Error::other(format!(
                            "read prompt: {e}"
                        ))));
                    }
                }
            }
        };
        Ok(match result {
            CookedRead::Line(line) => PromptEvent::Line(line),
            CookedRead::Eof => PromptEvent::Eof,
            CookedRead::Interrupt => PromptEvent::Interrupt,
        })
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
    async fn handle_command(&mut self, command: SlashCommand) -> CommandAction {
        match command {
            SlashCommand::Quit => CommandAction::Exit,
            SlashCommand::Help => {
                output::diag_line("commands: /help show this help, /clear print a separator, /new start a fresh conversation, /checkpoints list checkpoints, /quit exit (Ctrl-D also exits)");
                output::diag_line("history: /history [prefix] list recent prompts (optional prefix filter), /rerun [n] re-ask prompt n (default: last)");
                output::diag_line("display: /markdown [raw|plain|styled] set or show stdout rendering, /reasoning [on|off], /usage [on|off], /tools [on|off], /display show all");
                output::diag_line("keys: Tab or Right-arrow (at line end) accept the suggestion, Ctrl-Right or Alt-f accept one word, Up/Down browse history by prefix, Alt-b/Alt-f word motion, Alt-d/Alt-Backspace kill word, Alt-. insert history tail word, Ctrl-C cancels the running turn, Ctrl-C twice at the prompt quits, Ctrl-D quits");
                output::diag_line(
                    "notes: the full history stays in memory; prompt history is plain text on disk (see --history-file/--no-history); workflows run via `wf run --workflow` and the management subcommands",
                );
                CommandAction::Continue
            }
            SlashCommand::Clear => {
                output::diag_line("---");
                CommandAction::Continue
            }
            SlashCommand::Checkpoints => {
                let client = self.client.clone();
                match client
                    .get_json::<serde_json::Value>("/api/v1/checkpoints")
                    .await
                {
                    Ok(data) => {
                        let items = data
                            .get("data")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();
                        if items.is_empty() {
                            output::diag_line("no checkpoints");
                        } else {
                            for c in items {
                                let id = c.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                                let entity =
                                    c.get("entity_id").and_then(|v| v.as_str()).unwrap_or("?");
                                let status =
                                    c.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                                output::diag_line(&format!("{id}  {entity}  {status}"));
                            }
                        }
                    }
                    Err(err) => output::diag_line(&format!("checkpoint list failed: {err}")),
                }
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
            SlashCommand::History(prefix) => {
                if self.history.is_empty() {
                    output::diag_line("no history yet");
                    return CommandAction::Continue;
                }
                match prefix.as_deref() {
                    None => {
                        for (number, entry) in self.history.recent(20) {
                            output::diag_line(&format!("{number}: {entry}"));
                        }
                    }
                    Some(prefix) => {
                        let shown = self.history.filter_prefix(prefix);
                        if shown.is_empty() {
                            output::diag_line(&format!("no match for prefix '{prefix}'"));
                        } else {
                            for (number, entry) in shown {
                                output::diag_line(&format!("{number}: {entry}"));
                            }
                            // Cooked mode cannot draw the ghost dim after the
                            // cursor; print what the raw-mode prediction would
                            // complete to (same newest-match function).
                            if let Some(suffix) = self.history.suggest(prefix) {
                                output::diag_line(&format!("suggestion: {prefix}{suffix}"));
                            }
                        }
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
            SlashCommand::Markdown(mode) => {
                match mode {
                    None => output::diag_line(&format!(
                        "markdown: {} (raw|plain|styled)",
                        self.display.markdown.as_str()
                    )),
                    Some(raw) => match MarkdownMode::parse_opt(&raw) {
                        Some(parsed) => {
                            self.display.markdown = parsed;
                            output::diag_line(&format!(
                                "markdown: {}",
                                self.display.markdown.as_str()
                            ));
                        }
                        None => output::diag_line(&format!(
                            "unknown markdown mode '{raw}' (raw|plain|styled)"
                        )),
                    },
                }
                CommandAction::Continue
            }
            SlashCommand::Reasoning(toggle) => {
                self.apply_toggle("reasoning", toggle, |config, show| {
                    config.show_reasoning = show;
                });
                CommandAction::Continue
            }
            SlashCommand::Usage(toggle) => {
                self.apply_toggle("usage", toggle, |config, show| {
                    config.show_usage = show;
                });
                CommandAction::Continue
            }
            SlashCommand::Tools(toggle) => {
                self.apply_toggle("tools", toggle, |config, show| {
                    config.show_tools = show;
                });
                CommandAction::Continue
            }
            SlashCommand::Display => {
                output::diag_line(&format!("display: {}", self.display.describe()));
                CommandAction::Continue
            }
        }
    }

    /// Apply an `on`/`off` toggle to one stderr stream, or report its state
    /// when no value was given.
    fn apply_toggle(
        &mut self,
        name: &str,
        toggle: Option<String>,
        apply: impl FnOnce(&mut DisplayConfig, bool),
    ) {
        match toggle {
            None => {
                let show = match name {
                    "reasoning" => self.display.show_reasoning,
                    "usage" => self.display.show_usage,
                    _ => self.display.show_tools,
                };
                output::diag_line(&format!("{name}: {}", if show { "on" } else { "off" }));
            }
            Some(raw) => match parse_toggle(&raw) {
                Some(show) => {
                    apply(&mut self.display, show);
                    output::diag_line(&format!("{name}: {}", if show { "on" } else { "off" }));
                }
                None => output::diag_line(&format!("unknown value '{raw}' (on|off)")),
            },
        }
    }

    /// Resolve the session anchor and its transcript: an explicit
    /// `--session` id wins, `--resume` picks the most recent stored
    /// session, otherwise a fresh anchor starts empty. Restore problems
    /// fall back to an empty transcript without failing startup.
    async fn restore_session(
        client: &WfClient,
        cli: &wf_cli_shared::args::Cli,
    ) -> (String, Transcript) {
        let mut transcript = Transcript::new();
        if let Some(id) = cli.session.clone() {
            let stored = Self::load_history(client, &id).await;
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
            match Self::load_latest(client).await {
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
    async fn load_history(client: &WfClient, id: &str) -> Vec<wf_types::message::Message> {
        let value: Result<serde_json::Value, _> = client
            .get_json(&format!("/api/v1/agent-loops/{id}/conversation"))
            .await;
        value
            .ok()
            .and_then(|v| {
                v.get("items")
                    .and_then(|items| serde_json::from_value(items.clone()).ok())
                    .or_else(|| serde_json::from_value(v).ok())
            })
            .unwrap_or_default()
    }

    /// Most recently active stored session and its messages. Picks the
    /// newest loop summary and reads its conversation. Returns `None` when
    /// nothing resumable exists.
    async fn load_latest(client: &WfClient) -> Option<(String, Vec<wf_types::message::Message>)> {
        let summaries: serde_json::Value = client
            .get_json("/api/v1/agent-loops/summaries")
            .await
            .ok()?;
        let items = summaries
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .or_else(|| summaries.as_array().cloned())
            .unwrap_or_default();
        let mut newest: Option<(i64, String)> = None;
        for summary in &items {
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
        let stored = Self::load_history(client, &id).await;
        if stored.is_empty() {
            return None;
        }
        Some((id, stored))
    }

    /// File a completed turn into the transcript. The server persists the
    /// run ledger; the transcript only keeps memory for follow-up turns.
    /// Interrupted and failed turns never enter memory.
    fn record_completed_turn(&mut self, question: &str, finished: &FinishedTurn) {
        if !finished.outcome.completed {
            return;
        }
        self.transcript
            .push_pair(question, &finished.outcome.assistant_text);
    }

    /// Run one turn seeded with the session transcript. Returns `None`
    /// when the turn never started; the outcome tells whether the round
    /// counts as memory.
    async fn run_turn(&self, prompt: String) -> Option<FinishedTurn> {
        let params = TurnParams {
            agent: self.agent.clone(),
            model: self.model.clone(),
            approve_prefixes: Vec::new(),
            timeout_ms: self.timeout_ms,
            conversation: self.transcript.messages().to_vec(),
            kind: TurnKind::Agent { prompt },
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
    async fn pump_stream<S>(&self, execution_id: &str, mut stream: S) -> TurnOutcome
    where
        S: futures::Stream<Item = Result<ExecutionStreamEvent, Option<wf_client::ClientError>>>
            + Unpin,
    {
        let started = Instant::now();
        let mut renderer = TurnRenderer::<RealSink>::new(self.display);
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

/// Adapt server SSE frames into execution stream events, skipping the
/// stream metadata frame. Transport failures surface as `Some(..)`; the
/// pump treats them as turn-ending errors.
fn map_frames(
    stream: impl futures::Stream<Item = Result<wf_client::SseFrame, wf_client::ClientError>>
        + Send
        + 'static,
) -> impl futures::Stream<Item = Result<ExecutionStreamEvent, Option<wf_client::ClientError>>> + Unpin
{
    use futures::StreamExt;
    Box::pin(stream)
        .filter_map(|item| async move {
            match item {
                Err(e) => Some(Err(Some(e))),
                Ok(frame) => {
                    if frame.event == "metadata" {
                        return None;
                    }
                    match serde_json::from_value::<ExecutionStreamEvent>(frame.data) {
                        Ok(event) => Some(Ok(event)),
                        Err(_) => None,
                    }
                }
            }
        })
        .boxed()
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
