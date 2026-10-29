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
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;

use wf_client::{WfClient, WorkflowNestedAgentLoop};
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

/// Heartbeat interval for the running window title: one frame flip per
/// tick proves the process is alive, a frozen frame signals a stall.
const TITLE_HEARTBEAT: Duration = Duration::from_secs(1);

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
    /// Server-side agent loop id of the most recent turn, when known.
    current_loop_id: Option<String>,
    /// Workflow execution attached via `/loops` for nested agent-loop
    /// operations. Set only explicitly so plain agent turns never
    /// misroute their loop-only submits.
    nested_execution_id: Option<String>,
    /// Sticky nested node selection: auto-set when a listing shows a
    /// single target, otherwise set by an explicit `--node` submit.
    nested_selected_node: Option<String>,
    /// Last `/loops` snapshot backing the single-target default.
    nested_loops: Vec<WorkflowNestedAgentLoop>,
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
    /// Recap ticket: a late side-call answer whose ticket moved on is
    /// dropped instead of sticking to a newer turn or session.
    recap_ticket: u64,
    /// Window-title heartbeat frame index, advanced once per tick while a
    /// turn runs. A frozen frame in the title signals a stalled process.
    title_frame: usize,
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
        let turn_started = Instant::now();
        let ticket = self.note_turn_started();
        let progress_enabled = Self::terminal_progress_enabled();
        if progress_enabled {
            eprint!("\x1b]9;4;3\x07");
            let _ = std::io::stderr().flush();
        }
        if let Some(finished) = self.run_turn(question.clone()).await {
            self.record_completed_turn(&question, &finished);
            self.maybe_auto_recap(ticket, turn_started.elapsed(), &finished)
                .await;
        }
        if progress_enabled {
            eprint!("\x1b]9;4;0\x07");
            let _ = std::io::stderr().flush();
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
        let history_file_config = input::load_history_file_config();
        let history_limit = input::resolve_history_limit(history_file_config.as_ref());
        let history = match input::resolve_history_path(cli.history_file.as_deref(), cli.no_history)
        {
            Some(path) => History::load_with_limit(path, history_limit),
            None => History::with_limit(history_limit),
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
            current_loop_id: None,
            nested_execution_id: None,
            nested_selected_node: None,
            nested_loops: Vec::new(),
            raw_prompt,
            prev_prompt: None,
            suggest: Arc::new(SuggestEngine::new()),
            broker: Arc::new(SuggestBroker::new()),
            cancel_tx: tokio::sync::watch::channel(false).0,
            cancel_rx: tokio::sync::watch::channel(false).1,
            recap_ticket: 0,
            title_frame: 0,
        })
    }

    /// Run the prompt loop until EOF or a quit command.
    pub async fn run(mut self) -> CliResult<()> {
        self.set_terminal_title();
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
            skills: Vec::new(),
            workflows: Vec::new(),
            project_root: std::env::current_dir().ok(),
            suggestion_display_limit: self.display.suggestion_display_limit,
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
                output::diag_line("commands: /help show this help, /clear print a separator, /new start a fresh conversation, /checkpoints list checkpoints, /recap show a one-line recap of the turn, /quit exit (Ctrl-D also exits)");
                output::diag_line("sessions: /sessions [filter] list recent server sessions (newest first, id · title · status · age), /resume [id-or-filter] continue a session (bare resumes latest), /rename <title> rename current session, /delete [id-or-filter] delete a session (bare deletes current)");
                output::diag_line("history: /history [prefix] list recent prompts (optional prefix filter), /rerun [n] re-ask prompt n (default: last)");
                output::diag_line("display: /markdown [raw|plain|styled] set or show stdout rendering, /reasoning [on|off], /usage [on|off], /tools [on|off], /display show all");
                output::diag_line("nested loops: /loops [execution] list workflow nested targets, /steer [--node <id>] <text> steering input, /followup [--node <id>] <text> follow-up input, /append-mode [--node <id>] steering|followup, /convert-pending [--node <id>]");
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
                self.nested_execution_id = None;
                self.nested_selected_node = None;
                self.nested_loops = Vec::new();
                self.note_turn_started();
                self.set_terminal_title();
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
            SlashCommand::Recap => {
                self.request_recap_now().await;
                CommandAction::Continue
            }
            SlashCommand::Steer { node, text } => {
                self.submit_loop_text(node, text, "steering").await;
                CommandAction::Continue
            }
            SlashCommand::Followup { node, text } => {
                self.submit_loop_text(node, text, "followup").await;
                CommandAction::Continue
            }
            SlashCommand::AppendMode { node, mode } => {
                let mode_str = mode.unwrap_or_else(|| "followup".to_string());
                if mode_str != "steering" && mode_str != "followup" {
                    output::diag_line("usage: /append-mode [--node <id>] steering|followup");
                    return CommandAction::Continue;
                }
                self.set_loop_append_mode(node, mode_str).await;
                CommandAction::Continue
            }
            SlashCommand::ConvertPending { node } => {
                self.convert_loop_pending(node).await;
                CommandAction::Continue
            }
            SlashCommand::Loops(execution) => {
                self.list_nested_loops(execution).await;
                CommandAction::Continue
            }
            SlashCommand::Sessions(filter) => {
                self.list_sessions(filter).await;
                CommandAction::Continue
            }
            SlashCommand::Resume(target) => {
                self.resume_to(target).await;
                CommandAction::Continue
            }
            SlashCommand::Rename(title) => {
                self.rename_current(title).await;
                CommandAction::Continue
            }
            SlashCommand::Delete(target) => {
                self.delete_session(target).await;
                CommandAction::Continue
            }
        }
    }

    /// Resolve the nested submit target: an explicit `--node` wins, then
    /// the sticky selection, then the single entry of the last `/loops`
    /// snapshot. `Err` carries the user-facing reason for the diag line.
    fn resolve_nested_node(&self, explicit: Option<String>) -> Result<String, String> {
        if let Some(node) = explicit {
            return Ok(node);
        }
        if let Some(selected) = self.nested_selected_node.clone() {
            return Ok(selected);
        }
        if self.nested_loops.len() == 1 {
            return Ok(self.nested_loops[0].node_id.clone());
        }
        if self.nested_loops.is_empty() {
            return Err("no nested target selected; run /loops <execution> first".to_string());
        }
        Err("multiple nested targets; pass --node <id> (see /loops)".to_string())
    }

    /// Keep a failed submit text in prompt history so it can be retried
    /// via history recall instead of being silently dropped.
    fn keep_failed_input(&mut self, text: &str) {
        self.history.push(text);
    }

    /// Submit steering/follow-up text to a nested workflow node when a
    /// node is given or an execution is attached; otherwise fall back to
    /// the current loop-only path. Failures only diag and keep the text.
    async fn submit_loop_text(&mut self, node: Option<String>, text: String, mode: &str) {
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
    async fn set_loop_append_mode(&mut self, node: Option<String>, mode: String) {
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
    async fn convert_loop_pending(&mut self, node: Option<String>) {
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

    /// List the nested agent-loop targets of the attached execution (or
    /// attach to the given one first). Refreshes the snapshot and keeps
    /// the selection minimal: a single target is auto-selected, a stale
    /// selection is dropped, multiple targets need an explicit `--node`.
    async fn list_nested_loops(&mut self, execution: Option<String>) {
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

    /// A new turn (or session) moves the recap ticket so a late side-call
    /// answer can never stick to it.
    fn note_turn_started(&mut self) -> u64 {
        self.recap_ticket = self.recap_ticket.wrapping_add(1);
        self.recap_ticket
    }

    /// Write the terminal window title. While a turn runs the title is
    /// prefixed with an alternating heartbeat glyph; when idle the plain
    /// title is written.
    fn write_terminal_title(&self, running_frame: Option<usize>) {
        let base = format!("WF Agent - {}", self.anchor);
        let title = tui_terminal::terminal::state::running_title(
            &base,
            running_frame.unwrap_or(0),
            running_frame.is_some(),
        );
        eprint!(
            "\x1b]0;{}\x07",
            tui_terminal::terminal::state::sanitize_title(&title)
        );
        let _ = std::io::stderr().flush();
    }

    /// Set the terminal window title for this session.
    fn set_terminal_title(&self) {
        self.write_terminal_title(None);
    }

    fn terminal_progress_enabled() -> bool {
        if let Ok(value) = std::env::var("WF_TUI_TERMINAL_PROGRESS") {
            return matches!(value.as_str(), "1" | "true" | "TRUE" | "yes");
        }
        let Some(path) = wf_cli_shared::app_config::config_file_path() else {
            return false;
        };
        match wf_cli_shared::app_config::ConfigManager::load(&path) {
            Ok(manager) => manager.config().terminal.terminal_progress_enabled(),
            Err(_) => false,
        }
    }

    /// Whether automatic recap notes are on. The file is optional: absent
    /// or unreadable means on, matching the web default for old servers.
    fn recap_enabled() -> bool {
        let Some(path) = wf_cli_shared::app_config::config_file_path() else {
            return true;
        };
        match wf_cli_shared::app_config::ConfigManager::load(&path) {
            Ok(manager) => manager.config().behavior.recap_enabled(),
            Err(_) => true,
        }
    }

    /// Read one screen-only recap over HTTP. `None` is silence: nothing
    /// worth saying, side call failed, no profile, or the ticket moved on.
    async fn fetch_recap(&self, ticket: u64) -> Option<String> {
        let profile = self.model.clone().unwrap_or_default();
        if profile.trim().is_empty() {
            return None;
        }
        let body = serde_json::json!({
            "session_id": self.anchor,
            "profile_id": profile,
        });
        let response: Result<serde_json::Value, _> =
            self.client.post_json("/api/v1/chat/recap", &body).await;
        if ticket != self.recap_ticket {
            return None;
        }
        let text = response
            .ok()?
            .get("data")?
            .get("text")?
            .as_str()?
            .to_string();
        if text.trim().is_empty() {
            return None;
        }
        Some(text)
    }

    /// Manual `/recap`: same read face, bypasses the automatic gate and
    /// the display switch. Silence stays silent.
    async fn request_recap_now(&mut self) {
        let ticket = self.note_turn_started();
        if let Some(text) = self.fetch_recap(ticket).await {
            output::diag_line(&format!("※ recap: {text}"));
        }
    }

    /// Automatic recap after a completed turn. Only normal completions
    /// qualify; failures and interrupts already have their own notice.
    async fn maybe_auto_recap(&self, ticket: u64, elapsed: Duration, finished: &FinishedTurn) {
        if !finished.outcome.completed {
            return;
        }
        let reply_rows = reply_rows(&finished.outcome.assistant_text);
        let screen_rows = terminal_rows();
        if !wf_cli_shared::recap::wants_recap(
            Some(elapsed),
            false,
            reply_rows,
            screen_rows,
            Self::recap_enabled(),
            false,
        ) {
            return;
        }
        if let Some(text) = self.fetch_recap(ticket).await {
            output::diag_line(&format!("※ recap: {text}"));
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

    /// Most recently active stored session and its messages. Prefers the
    /// server-side latest session anchor (newest persisted conversation);
    /// falls back to scanning summaries when the anchor endpoint is
    /// unavailable. Returns `None` when nothing resumable exists.
    async fn load_latest(client: &WfClient) -> Option<(String, Vec<wf_types::message::Message>)> {
        if let Ok(value) = client
            .get_json::<serde_json::Value>("/api/v1/agent-loops/latest-session-anchor")
            .await
        {
            let id = value
                .get("data")
                .and_then(|v| v.as_str())
                .or_else(|| value.as_str())
                .map(str::to_string);
            if let Some(id) = id {
                let stored = Self::load_history(client, &id).await;
                if !stored.is_empty() {
                    return Some((id, stored));
                }
            }
        }

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
    /// Interrupted and failed turns never enter memory. On success the
    /// session anchor tracks the latest server loop so `--session <anchor>`
    /// stays resumable after the process exits.
    fn record_completed_turn(&mut self, question: &str, finished: &FinishedTurn) {
        if !finished.outcome.completed {
            return;
        }
        self.transcript
            .push_pair(question, &finished.outcome.assistant_text);
        if let Some(id) = self.current_loop_id.clone() {
            if self.anchor != id {
                self.anchor = id;
                self.set_terminal_title();
            }
        }
    }

    /// One fetched session row for the interactive `/sessions` listing.
    async fn fetch_session_rows(client: &WfClient) -> Vec<MiniSessionRow> {
        let value: Result<serde_json::Value, _> = client
            .get_json("/api/v1/agent-loops/summaries?order=desc&limit=50")
            .await;
        let items = value
            .ok()
            .and_then(|v| {
                v.get("items")
                    .and_then(|items| items.as_array().cloned())
                    .or_else(|| v.as_array().cloned())
            })
            .unwrap_or_default();
        let mut rows: Vec<MiniSessionRow> = items
            .into_iter()
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?.to_string();
                if id.trim().is_empty() {
                    return None;
                }
                Some(MiniSessionRow {
                    start_ms: item.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0),
                    id,
                    title: item
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|title| !title.is_empty())
                        .map(str::to_string),
                    status: item
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?")
                        .to_string(),
                })
            })
            .collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.start_ms));
        rows.truncate(50);
        rows
    }

    /// List recent server sessions newest-first, filtered by a substring
    /// over id, title and status. Empty servers and empty filters report
    /// explicitly instead of printing nothing.
    async fn list_sessions(&self, filter: Option<String>) {
        let rows = Self::fetch_session_rows(&self.client).await;
        if rows.is_empty() {
            output::diag_line("no recent sessions yet — send a turn first");
            return;
        }
        let needle = filter.as_deref().unwrap_or_default().trim().to_lowercase();
        let shown: Vec<&MiniSessionRow> = if needle.is_empty() {
            rows.iter().take(20).collect()
        } else {
            rows.iter()
                .filter(|row| {
                    row.id.to_lowercase().contains(&needle)
                        || row.status.to_lowercase().contains(&needle)
                        || row
                            .title
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&needle)
                })
                .take(20)
                .collect()
        };
        if shown.is_empty() {
            output::diag_line(&format!(
                "no match for '{}' ({} sessions)",
                filter.unwrap_or_default(),
                rows.len()
            ));
            return;
        }
        for row in shown {
            let label = row.title.as_deref().unwrap_or("-");
            output::diag_line(&format!(
                "{} · {} · {} · {}",
                short_session_id(&row.id),
                label,
                row.status,
                mini_relative_age(row.start_ms)
            ));
        }
        if needle.is_empty() && rows.len() > 20 {
            output::diag_line(&format!("… {} more (pass a filter)", rows.len() - 20));
        }
    }

    /// Continue a session interactively: bare resumes the latest session,
    /// an argument resumes an exact id or the newest id/title/status match.
    /// The transcript is replaced and the anchor tracks the resumed loop so
    /// the next turn seeds from its conversation.
    async fn resume_to(&mut self, target: Option<String>) {
        let needle = target.as_deref().map(str::trim).unwrap_or_default();
        if needle.is_empty() {
            match Self::load_latest(&self.client).await {
                Some((id, stored)) => self.switch_transcript(id, stored),
                None => output::diag_line("no previous session found; staying here"),
            }
            return;
        }
        let stored = Self::load_history(&self.client, needle).await;
        if !stored.is_empty() {
            self.switch_transcript(needle.to_string(), stored);
            return;
        }
        let rows = Self::fetch_session_rows(&self.client).await;
        let lower = needle.to_lowercase();
        let hit = rows
            .iter()
            .find(|row| {
                row.id.to_lowercase().contains(&lower)
                    || row.status.to_lowercase().contains(&lower)
                    || row
                        .title
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&lower)
            })
            .map(|row| row.id.clone());
        match hit {
            Some(id) => {
                let stored = Self::load_history(&self.client, &id).await;
                if stored.is_empty() {
                    output::diag_line(&format!("session '{id}' has no saved messages"));
                } else {
                    self.switch_transcript(id, stored);
                }
            }
            None => output::diag_line(&format!("no session matches '{needle}'")),
        }
    }

    /// Replace the in-memory transcript with a resumed conversation.
    fn switch_transcript(&mut self, id: String, stored: Vec<wf_types::message::Message>) {
        let mut transcript = Transcript::new();
        let restored = transcript.restore(stored);
        self.transcript = transcript;
        self.anchor = id.clone();
        self.current_loop_id = None;
        self.nested_execution_id = None;
        self.nested_selected_node = None;
        self.nested_loops = Vec::new();
        self.note_turn_started();
        self.set_terminal_title();
        output::diag_line(&format!("resumed session {id} ({restored} messages) — continue chatting"));
    }

    /// Rename the current session. Fresh `mini-*` anchors without a server
    /// turn have nothing to rename yet.
    async fn rename_current(&mut self, title: Option<String>) {
        let Some(title) = title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) else {
            output::diag_line("usage: /rename <title>");
            return;
        };
        let target = self
            .current_loop_id
            .clone()
            .or_else(|| {
                (!self.anchor.starts_with("mini-"))
                    .then(|| self.anchor.clone())
            });
        let Some(id) = target else {
            output::diag_line("no server session yet — send a turn first, then /rename");
            return;
        };
        match self.client.rename_agent_loop(&id, &title).await {
            Ok(_) => output::diag_line(&format!("renamed session {id} to {title}")),
            Err(err) => output::diag_line(&format!("rename failed: {err}")),
        }
    }

    /// Delete a session and its conversation. Bare deletes the current
    /// session anchor (or latest loop); an argument deletes an exact id or
    /// the newest filter match. Deleting the current session clears the
    /// transcript and starts a fresh anchor.
    async fn delete_session(&mut self, target: Option<String>) {
        let needle = target.as_deref().map(str::trim).unwrap_or_default();
        let id = if needle.is_empty() {
            self.current_loop_id.clone().unwrap_or_else(|| self.anchor.clone())
        } else {
            let stored = Self::load_history(&self.client, needle).await;
            if !stored.is_empty() {
                needle.to_string()
            } else {
                let rows = Self::fetch_session_rows(&self.client).await;
                let lower = needle.to_lowercase();
                match rows
                    .iter()
                    .find(|row| {
                        row.id.to_lowercase().contains(&lower)
                            || row.status.to_lowercase().contains(&lower)
                            || row
                                .title
                                .as_deref()
                                .unwrap_or_default()
                                .to_lowercase()
                                .contains(&lower)
                    })
                    .map(|row| row.id.clone())
                {
                    Some(id) => id,
                    None => {
                        output::diag_line(&format!("no session matches '{needle}'"));
                        return;
                    }
                }
            }
        };
        if id.starts_with("mini-") && self.current_loop_id.is_none() {
            output::diag_line("no server session yet — nothing to delete");
            return;
        }
        match self.client.delete_agent_loop(&id).await {
            Ok(_) => {
                output::diag_line(&format!("deleted session {id}"));
                if id == self.anchor || Some(&id) == self.current_loop_id.as_ref() {
                    self.transcript.clear();
                    self.anchor = format!("mini-{}", wf_common::generate_id());
                    self.current_loop_id = None;
                    self.nested_execution_id = None;
                    self.nested_selected_node = None;
                    self.nested_loops = Vec::new();
                    self.note_turn_started();
                    self.set_terminal_title();
                    output::diag_line(&format!(
                        "started a new conversation (session {})",
                        self.anchor
                    ));
                }
            }
            Err(err) => output::diag_line(&format!("delete failed: {err}")),
        }
    }

    /// Run one turn seeded with the session transcript. Returns `None`
    /// when the turn never started; the outcome tells whether the round
    /// counts as memory.
    async fn run_turn(&mut self, prompt: String) -> Option<FinishedTurn> {
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
    async fn pump_stream<S>(&mut self, execution_id: &str, mut stream: S) -> TurnOutcome
    where
        S: futures::Stream<Item = Result<ExecutionStreamEvent, Option<wf_client::ClientError>>>
            + Unpin,
    {
        let started = Instant::now();
        let mut renderer = TurnRenderer::<RealSink>::new(self.display);
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
        // Back to the plain session title regardless of how the turn ended.
        self.write_terminal_title(None);
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

/// Visible rows of the reply: physical lines folded to the terminal
/// width, so a long single line still counts as over-screen output.
fn reply_rows(text: &str) -> usize {
    let cols = crate::editor::terminal_cols().max(20) as usize;
    text.lines()
        .map(|line| {
            let width = line.chars().count().max(1);
            width.div_ceil(cols).max(1)
        })
        .sum::<usize>()
        .max(1)
}

/// Terminal height in rows; 24 on failure so the gate still decides.
fn terminal_rows() -> u16 {
    crossterm::terminal::size()
        .map(|(_, rows)| rows)
        .unwrap_or(24)
}

/// One row of the `/sessions` listing: server id plus its title, status
/// and start time for relative-age display.
#[derive(Debug, Clone)]
struct MiniSessionRow {
    id: String,
    title: Option<String>,
    status: String,
    start_ms: i64,
}

/// Short session id for diag lines, matching the full TUI short id.
fn short_session_id(id: &str) -> String {
    if id.chars().count() <= 8 {
        id.to_string()
    } else {
        format!("{}...", id.chars().take(7).collect::<String>())
    }
}

/// Relative age label for a start timestamp, shared with the full TUI
/// recent pane so both CLIs read the same (`now`, `5m`, `2h`, `3d`).
fn mini_relative_age(start_ms: i64) -> String {
    if start_ms <= 0 {
        return "-".to_string();
    }
    let now_ms = wf_common::now().max(0) as u64;
    let delta_ms = now_ms.saturating_sub(start_ms.max(0) as u64);
    let minutes = delta_ms / 60_000;
    if minutes < 1 {
        return "now".to_string();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days}d");
    }
    format!("{}mo", days / 30)
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
