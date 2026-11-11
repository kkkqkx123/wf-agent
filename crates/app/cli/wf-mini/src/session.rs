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
use crate::input::{self, AttachArgs, History, ImageArgs, LineReader, PluginSlash, SlashCommand};
use crate::output::{self};
use crate::suggest::{PluginCompletionSource, SuggestBroker, SuggestEngine};
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
    pub(crate) fn note_press(&mut self, now: Instant) -> bool {
        let quit = self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) <= PROMPT_QUIT_WINDOW);
        self.last = Some(now);
        quit
    }

    pub(crate) fn reset(&mut self) {
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
    /// Cached rows of the last `/sessions` listing (display order) backing
    /// numeric `/resume <n>` and `/delete <n>` targets.
    last_session_rows: Vec<MiniSessionRow>,
    /// Session id armed by a first `/delete`: repeating the same command
    /// confirms, any other outcome clears it.
    pending_delete: Option<String>,
    /// Session-level structured agent context: set once via `--context` /
    /// `--context-file` or `/context`, carried on every subsequent turn.
    session_context: serde_json::Map<String, serde_json::Value>,
    /// Merged plugin slash names resolved before builtin misses fall
    /// through to the prompt: machine-local entries first, then the
    /// server snapshot. Empty keeps the builtin-only behavior.
    plugin_slash: Vec<PluginSlash>,
    /// Merged plugin completion sources backing the suggestion pipeline,
    /// local first then snapshot, exact duplicates dropped.
    plugin_completions: Vec<PluginCompletionSource>,
    /// Server snapshot slash entries, kept to rebuild the merged
    /// registry when local entries change.
    snapshot_slash: Vec<PluginSlash>,
    /// Server snapshot completion sources.
    snapshot_completions: Vec<PluginCompletionSource>,
    /// Machine-local slash entries, shadowing same-name snapshot entries.
    local_slash: Vec<PluginSlash>,
    /// Machine-local completion sources.
    local_completions: Vec<PluginCompletionSource>,
    /// Merged settings rows with visitor-config values under
    /// `plugins.<plugin>.<name>`; `/display` renders them grouped.
    plugin_settings: Vec<wf_types::cli::CliSettingField>,
    snapshot_settings: Vec<wf_types::cli::CliSettingField>,
    local_settings: Vec<wf_types::cli::CliSettingField>,
    plugin_setting_values: std::collections::HashMap<String, String>,
    /// Merged tool summary templates; stored for parity with the full
    /// shell. Mini renders tool lines as plain text for now.
    plugin_tool_summaries: Vec<wf_types::cli::CliToolSummary>,
    snapshot_tool_summaries: Vec<wf_types::cli::CliToolSummary>,
    local_tool_summaries: Vec<wf_types::cli::CliToolSummary>,
    /// Merged Markdown rules; stored for parity. Mini prints plain
    /// lines for now.
    plugin_markdown_rules: Vec<wf_types::cli::CliMarkdownRule>,
    snapshot_markdown_rules: Vec<wf_types::cli::CliMarkdownRule>,
    local_markdown_rules: Vec<wf_types::cli::CliMarkdownRule>,
    /// Snapshot revision behind the snapshot sets; equal revisions skip
    /// the rebuild. The local fingerprint has its own counter.
    snapshot_revision: u64,
    local_revision: u64,
    /// Image files staged by `/image` for the next turn. Taken when the
    /// turn starts; a turn that never starts hands them back.
    staged_images: Vec<wf_types::message::UserImageInput>,
    /// Text files staged for the next turn. Taken when the turn starts;
    /// staged with `/attach` and listed with bare `/attach`.
    staged_attachments: Vec<wf_types::message::UserTextAttachment>,
    /// Render detail applied to newly staged images (`/image-detail`).
    /// Empty means the provider default; kept across turns until changed.
    staged_image_detail: Option<wf_types::message::ImageDetail>,
}

/// Merge one contribution kind with local entries first: the first entry
/// for a key wins, so machine-local plugins shadow same-key snapshot
/// entries without failing.
fn merge_by_key<T: Clone>(local: &[T], snapshot: &[T], key: impl Fn(&T) -> String) -> Vec<T> {
    let mut merged: Vec<T> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for item in local.iter().chain(snapshot.iter()) {
        let id = key(item);
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        merged.push(item.clone());
    }
    merged
}

/// What a slash command asks the loop to do.
enum CommandAction {
    Continue,
    Exit,
    /// Run this prompt as a question (used by `/rerun`).
    Ask(String),
}

impl NativeSession {
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
                    let (chained, takeover) = self.run_cli_input_chain(raw.trim());
                    if let Some(echo) = takeover {
                        if !echo.is_empty() {
                            output::diag_line(&echo);
                        }
                        continue;
                    }
                    let trimmed = chained.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Some(command) = input::parse_command_with(&trimmed, &self.plugin_slash) {
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
    pub(crate) async fn read_prompt_raw(&mut self) -> CliResult<PromptEvent> {
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
    pub(crate) async fn read_prompt_cooked(&mut self) -> CliResult<PromptEvent> {
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
    pub(crate) async fn discard_stale_lines(&self) {
        let dropped = self.lines.lock().await.drain();
        if dropped > 0 {
            output::diag_line(&format!(
                "cleared {dropped} stale input line{} (typed while busy)",
                if dropped == 1 { "" } else { "s" }
            ));
        }
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
    iterations: Option<u32>,
    assistant_text: String,
}

impl TurnOutcome {
    pub(crate) fn finished(
        completed: bool,
        assistant_text: String,
        iterations: Option<u32>,
    ) -> Self {
        Self {
            completed,
            interrupted: false,
            iterations,
            assistant_text,
        }
    }

    pub(crate) fn interrupted(assistant_text: String) -> Self {
        Self {
            completed: false,
            interrupted: true,
            iterations: None,
            assistant_text,
        }
    }

    pub(crate) fn uncompleted(assistant_text: String) -> Self {
        Self {
            completed: false,
            interrupted: false,
            iterations: None,
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

/// One parsed session filter token: a phrase requires a normalized
/// exact hit, a word a case-insensitive substring hit. All tokens of a
/// query must match (AND).
#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionToken {
    phrase: bool,
    value: String,
}

/// Split a filter into tokens, honoring double quotes for exact phrases.
/// Unbalanced quotes fall back to plain whitespace splitting.
fn parse_session_tokens(query: &str) -> Vec<SessionToken> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in trimmed.chars() {
        if ch == '"' {
            if in_quote {
                let value = current.trim().to_string();
                if !value.is_empty() {
                    tokens.push(SessionToken {
                        phrase: true,
                        value,
                    });
                }
                current.clear();
                in_quote = false;
            } else {
                let value = current.trim().to_string();
                if !value.is_empty() {
                    tokens.push(SessionToken {
                        phrase: false,
                        value,
                    });
                }
                current.clear();
                in_quote = true;
            }
            continue;
        }
        if !in_quote && ch.is_whitespace() {
            let value = current.trim().to_string();
            if !value.is_empty() {
                tokens.push(SessionToken {
                    phrase: false,
                    value,
                });
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }
    if in_quote {
        // Unbalanced quote: the quote characters were literal typing,
        // split the whole query on whitespace instead.
        return trimmed
            .split_whitespace()
            .map(|part| SessionToken {
                phrase: false,
                value: part.to_string(),
            })
            .collect();
    }
    let value = current.trim().to_string();
    if !value.is_empty() {
        tokens.push(SessionToken {
            phrase: false,
            value,
        });
    }
    tokens
}

/// Lowercase and collapse whitespace so `"deploy  api"` matches
/// titles with different spacing.
fn normalize_session_text(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Strip a leading `--named` flag from a `/sessions`/`/resume`/`/delete`
/// filter. Returns the flag and the remaining query.
fn strip_named_flag(filter: Option<String>) -> (bool, Option<String>) {
    let Some(raw) = filter else {
        return (false, None);
    };
    let trimmed = raw.trim();
    if trimmed == "--named" {
        return (true, None);
    }
    if let Some(rest) = trimmed.strip_prefix("--named ") {
        let rest = rest.trim();
        if rest.is_empty() {
            return (true, None);
        }
        return (true, Some(rest.to_string()));
    }
    if trimmed.is_empty() {
        return (false, None);
    }
    (false, Some(trimmed.to_string()))
}

/// Score of one row for one token: title hits rank first, then id,
/// then status. `None` means no hit.
fn session_token_score(row: &MiniSessionRow, token: &SessionToken) -> Option<u32> {
    if token.phrase {
        let needle = normalize_session_text(&token.value);
        if needle.is_empty() {
            return Some(0);
        }
        if let Some(title) = row.title.as_deref() {
            if normalize_session_text(title).contains(&needle as &str) {
                return Some(0);
            }
        }
        if normalize_session_text(&row.id).contains(&needle as &str) {
            return Some(1);
        }
        if normalize_session_text(&row.status).contains(&needle as &str) {
            return Some(2);
        }
        return None;
    }
    let needle = token.value.to_lowercase();
    if let Some(title) = row.title.as_deref() {
        if title.to_lowercase().contains(&needle as &str) {
            return Some(0);
        }
    }
    if row.id.to_lowercase().contains(&needle as &str) {
        return Some(1);
    }
    if row.status.to_lowercase().contains(&needle as &str) {
        return Some(2);
    }
    None
}

/// Total score of one row for all tokens, or `None` when any token
/// misses (AND semantics).
fn session_row_score(row: &MiniSessionRow, tokens: &[SessionToken]) -> Option<u32> {
    let mut total = 0u32;
    for token in tokens {
        total += session_token_score(row, token)?;
    }
    Some(total)
}

/// Filter rows by all tokens and rank title hits before id/status hits.
/// An empty query keeps the fetch order (newest first). The sort is
/// stable so ties keep the fetch order.
fn filter_session_rows<'a>(
    rows: Vec<&'a MiniSessionRow>,
    tokens: &[SessionToken],
) -> Vec<&'a MiniSessionRow> {
    if tokens.is_empty() {
        return rows;
    }
    let mut scored: Vec<(&'a MiniSessionRow, u32)> = rows
        .into_iter()
        .filter_map(|row| session_row_score(row, tokens).map(|score| (row, score)))
        .collect();
    scored.sort_by_key(|(_, score)| *score);
    scored.into_iter().map(|(row, _)| row).collect()
}

/// Resolve a numeric `/resume <n>` or `/delete <n>` target against the
/// last `/sessions` listing (1-based display order).
fn resolve_numbered_target(target: &str, last: &[MiniSessionRow]) -> Option<String> {
    let index: usize = target.trim().parse().ok()?;
    if index == 0 {
        return None;
    }
    last.get(index - 1).map(|row| row.id.clone())
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

mod commands;
mod loops_media;
mod plugin;
mod recap;
mod store;
mod turn_exec;

#[cfg(test)]
mod test;
