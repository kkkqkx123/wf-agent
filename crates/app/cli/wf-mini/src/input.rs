//! Line-discipline input: dual pipelines, slash commands, history.
//!
//! The process reads stdin through exactly one source, chosen once at
//! startup by stdin TTY state and never switched mid-run:
//! - pipe pipeline (stdin not a TTY): one pump thread forwards raw byte
//!   chunks into a channel; [`LineReader`] assembles cooked lines.
//! - TTY pipeline (stdin is a TTY): the crossterm event stream is the only
//!   stdin reader; raw prompts consume key events, cooked prompts and
//!   approvals assemble lines from key events up to Enter.
//!
//! Two consumers take turns on the active source, never concurrently:
//! cooked [`LineReader`] use (approvals, piped stdin, TTY cooked fallback)
//! and the raw-mode prompt editor ([`crate::editor`]) which borrows the
//! stream or receiver for one prompt and hands it back afterwards.

use std::collections::VecDeque;
use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use tokio::sync::mpsc;

/// Maximum history entries retained in memory.
const HISTORY_LIMIT: usize = 200;

/// Slash commands accepted at the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Quit,
    Help,
    Clear,
    New,
    History(Option<String>),
    Rerun(Option<usize>),
    Markdown(Option<String>),
    Reasoning(Option<String>),
    Usage(Option<String>),
    Tools(Option<String>),
    Display,
}

/// Parse a trimmed input line into a slash command, if it is one.
///
/// `/rerun` takes an optional 1-based history number (`/rerun 3`); a
/// malformed variant is plain text, like any other unknown command.
pub fn parse_command(line: &str) -> Option<SlashCommand> {
    let mut tokens = line.split_whitespace();
    match tokens.next()? {
        "/quit" | "/exit" | ":q" if tokens.next().is_none() => Some(SlashCommand::Quit),
        "/help" | "/?" if tokens.next().is_none() => Some(SlashCommand::Help),
        "/clear" if tokens.next().is_none() => Some(SlashCommand::Clear),
        "/new" if tokens.next().is_none() => Some(SlashCommand::New),
        "/history" => match tokens.next() {
            None => Some(SlashCommand::History(None)),
            Some(prefix) if tokens.next().is_none() => {
                Some(SlashCommand::History(Some(prefix.to_string())))
            }
            _ => None,
        },
        "/rerun" => match tokens.next() {
            None => Some(SlashCommand::Rerun(None)),
            Some(number) => match number.parse::<usize>() {
                Ok(n) if n >= 1 && tokens.next().is_none() => Some(SlashCommand::Rerun(Some(n))),
                _ => None,
            },
        },
        "/markdown" => match tokens.next() {
            None => Some(SlashCommand::Markdown(None)),
            Some(mode) if tokens.next().is_none() => {
                Some(SlashCommand::Markdown(Some(mode.to_string())))
            }
            _ => None,
        },
        "/reasoning" => match tokens.next() {
            None => Some(SlashCommand::Reasoning(None)),
            Some(toggle) if tokens.next().is_none() => {
                Some(SlashCommand::Reasoning(Some(toggle.to_string())))
            }
            _ => None,
        },
        "/usage" => match tokens.next() {
            None => Some(SlashCommand::Usage(None)),
            Some(toggle) if tokens.next().is_none() => {
                Some(SlashCommand::Usage(Some(toggle.to_string())))
            }
            _ => None,
        },
        "/tools" => match tokens.next() {
            None => Some(SlashCommand::Tools(None)),
            Some(toggle) if tokens.next().is_none() => {
                Some(SlashCommand::Tools(Some(toggle.to_string())))
            }
            _ => None,
        },
        "/display" if tokens.next().is_none() => Some(SlashCommand::Display),
        _ => None,
    }
}

/// In-memory prompt history with consecutive-duplicate suppression,
/// optionally backed by a plain-text file (one prompt per line, shell
/// convention) so `/history` and `/rerun` span processes.
///
/// File writes are best-effort and immediate (every submit appends, so a
/// killed process loses nothing); failures never surface to the session.
#[derive(Debug, Default)]
pub struct History {
    entries: VecDeque<String>,
    file: Option<std::path::PathBuf>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the file tail (up to the memory cap) and keep appending to it.
    /// A missing or unreadable file reads as empty history.
    pub fn load(path: std::path::PathBuf) -> Self {
        let mut history = Self {
            entries: VecDeque::new(),
            file: Some(path.clone()),
        };
        if let Ok(text) = std::fs::read_to_string(&path) {
            let lines: Vec<&str> = text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect();
            let tail = lines.len().saturating_sub(HISTORY_LIMIT);
            for line in &lines[tail..] {
                history.push_memory(line.trim());
            }
        }
        history
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 1-based lookup for `/rerun n`.
    pub fn get(&self, number: usize) -> Option<&str> {
        self.entries.get(number.checked_sub(1)?).map(String::as_str)
    }

    /// Last `count` entries with their 1-based numbers, oldest first.
    pub fn recent(&self, count: usize) -> Vec<(usize, &str)> {
        let skip = self.entries.len().saturating_sub(count);
        self.entries
            .iter()
            .skip(skip)
            .enumerate()
            .map(|(offset, entry)| (skip + offset + 1, entry.as_str()))
            .collect()
    }

    /// Point-in-time copy for the raw editor and the suggestion strategies,
    /// oldest first. History only grows between prompts, so a snapshot
    /// taken when a prompt starts stays consistent for the whole prompt.
    pub fn snapshot(&self) -> Vec<String> {
        self.entries.iter().cloned().collect()
    }

    /// Inline-prediction suffix for `prefix`: the rest of the newest entry
    /// starting with `prefix` and strictly longer. Empty for empty prefixes,
    /// exact repeats and misses. Cooked mode cannot draw it dim after the
    /// cursor, so `/history [prefix]` lists the matches explicitly; the raw
    /// editor draws it dim instead — both read the same strategy layer.
    pub fn suggest(&self, prefix: &str) -> Option<String> {
        suggest_suffix(prefix, &self.entries)
    }

    /// Every entry starting with `prefix` (oldest first), backing
    /// `/history [prefix]`.
    pub fn filter_prefix(&self, prefix: &str) -> Vec<(usize, &str)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.starts_with(prefix))
            .map(|(offset, entry)| (offset + 1, entry.as_str()))
            .collect()
    }

    pub fn push(&mut self, line: &str) {
        let flattened = line.replace('\n', " ");
        let trimmed = flattened.trim();
        if trimmed.is_empty() {
            return;
        }
        if self.entries.back().map(String::as_str) == Some(trimmed) {
            return;
        }
        self.push_memory(trimmed);
        if let Some(path) = self.file.clone() {
            append_history_line(&path, trimmed);
        }
    }

    fn push_memory(&mut self, line: &str) {
        if self.entries.back().map(String::as_str) == Some(line) {
            return;
        }
        self.entries.push_back(line.to_string());
        if self.entries.len() > HISTORY_LIMIT {
            self.entries.pop_front();
        }
    }
}

/// Newest-first prefix suffix over any oldest-first entry list.
fn suggest_suffix(prefix: &str, entries: &VecDeque<String>) -> Option<String> {
    if prefix.is_empty() {
        return None;
    }
    entries
        .iter()
        .rev()
        .find(|entry| entry.starts_with(prefix) && entry.len() > prefix.len())
        .map(|entry| entry[prefix.len()..].to_string())
}

/// Append one line plus newline, creating parent dirs on first use.
/// Best-effort: every error is swallowed.
fn append_history_line(path: &std::path::Path, line: &str) {
    use std::io::Write as _;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Resolve the history file: explicit flag wins, then the environment,
/// then the state-dir default. Disabled (flag or env) reads as `None`.
/// No new dependencies: the default is derived from `XDG_STATE_HOME` or
/// `HOME` directly.
pub fn resolve_history_path(
    flag: Option<&std::path::Path>,
    no_history: bool,
) -> Option<std::path::PathBuf> {
    if no_history || env_flag_disabled("WF_MINI_NO_HISTORY") {
        return None;
    }
    if let Some(path) = flag {
        return Some(path.to_path_buf());
    }
    if let Ok(path) = std::env::var("WF_MINI_HISTORY_FILE") {
        if !path.trim().is_empty() {
            return Some(std::path::PathBuf::from(path));
        }
    }
    let base = std::env::var("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| std::path::PathBuf::from(home).join(".local").join("state"))
        })?;
    Some(base.join("wf").join("mini_history"))
}

fn env_flag_disabled(name: &str) -> bool {
    let raw = std::env::var(name).unwrap_or_default();
    if raw.trim().is_empty() {
        return false;
    }
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Spawn the pipe-pipeline stdin pump: one detached thread reads raw byte
/// chunks and forwards them into the channel. It is the only stdin reader
/// on the pipe path; nothing else in the process may touch fd 0.
///
/// The thread cannot be interrupted while blocked in `read`; it exits on
/// EOF, read error, or when the channel closes, and the OS reaps it at
/// process exit regardless. `blocking_send` is legal here: the thread was
/// not created by tokio.
fn spawn_stdin_pump(tx: mpsc::UnboundedSender<Vec<u8>>) {
    let _ = std::thread::Builder::new()
        .name("stdin-pump".into())
        .spawn(move || {
            use std::io::Read as _;
            let mut stdin = io::stdin().lock();
            let mut buf = [0u8; 1024];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        // Unbounded `send` is a plain sync method: it never
                        // blocks and works from a non-async thread.
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
}

struct PipeReader {
    rx: Option<mpsc::UnboundedReceiver<Vec<u8>>>,
    pending: Vec<u8>,
    finished: bool,
}

struct TtyReader {
    stream: Option<EventStream>,
    pending_lines: VecDeque<String>,
    partial: String,
    finished: bool,
}

enum Inner {
    Pipe(PipeReader),
    Tty(TtyReader),
}

/// Dual-pipeline line assembler: the single consumer of stdin while the raw
/// editor is not running.
///
/// Pipe path holds the byte channel; TTY path holds the crossterm event
/// stream and assembles cooked lines from key events up to Enter. The
/// channel is unbounded so the pump never blocks: backpressure here
/// would stall interrupt handling while buying nothing (bytes are tiny).
pub struct LineReader {
    inner: Inner,
}

impl LineReader {
    /// Choose the process-wide stdin source once. Call once per process;
    /// the session owns it. TTY stdin uses the event stream as the only
    /// reader (no pump thread competes for fd 0); pipes keep the byte pump.
    pub fn for_stdin() -> Self {
        if io::stdin().is_terminal() {
            Self {
                inner: Inner::Tty(TtyReader {
                    stream: Some(EventStream::new()),
                    pending_lines: VecDeque::new(),
                    partial: String::new(),
                    finished: false,
                }),
            }
        } else {
            let (tx, rx) = mpsc::unbounded_channel();
            spawn_stdin_pump(tx);
            Self {
                inner: Inner::Pipe(PipeReader {
                    rx: Some(rx),
                    pending: Vec::new(),
                    finished: false,
                }),
            }
        }
    }

    /// Wrap an existing channel. Tests use this to feed fake input without
    /// touching real stdin.
    #[cfg(test)]
    pub fn from_receiver(rx: mpsc::UnboundedReceiver<Vec<u8>>) -> Self {
        Self {
            inner: Inner::Pipe(PipeReader {
                rx: Some(rx),
                pending: Vec::new(),
                finished: false,
            }),
        }
    }

    /// Wrap a TTY reader without a live event stream. Tests use this for
    /// pipeline bookkeeping without touching real stdin state.
    #[cfg(test)]
    pub fn for_tty_test() -> Self {
        Self {
            inner: Inner::Tty(TtyReader {
                stream: None,
                pending_lines: VecDeque::new(),
                partial: String::new(),
                finished: false,
            }),
        }
    }

    /// True on the TTY event-stream pipeline.
    pub fn is_tty(&self) -> bool {
        matches!(self.inner, Inner::Tty(_))
    }

    /// Hand the event stream to the raw editor for one prompt. While it is
    /// gone, [`Self::next_line`] reports EOF without latching finished.
    pub fn take_event_stream(&mut self) -> Option<EventStream> {
        match &mut self.inner {
            Inner::Tty(tty) => tty.stream.take(),
            Inner::Pipe(_) => None,
        }
    }

    /// Take the event stream back after the editor finished.
    pub fn restore_event_stream(&mut self, stream: EventStream) {
        if let Inner::Tty(tty) = &mut self.inner {
            tty.stream = Some(stream);
        }
    }

    /// Wait for the next complete line. Returns `None` once stdin hit EOF
    /// and every buffered byte was consumed; an EOF with a pending
    /// unterminated line yields that line (shell convention for piped
    /// input without a trailing newline).
    pub async fn next_line(&mut self) -> Option<String> {
        match &mut self.inner {
            Inner::Pipe(pipe) => pipe_next_line(pipe).await,
            Inner::Tty(tty) => tty_next_line(tty).await,
        }
    }

    /// Drop every buffered but unconsumed byte, returning how many complete
    /// lines were dropped. Never blocks. Runs whenever consumption passes
    /// back to the prompt, so a late approval answer or a line typed
    /// mid-stream is never misread as the next prompt.
    pub fn drain(&mut self) -> usize {
        match &mut self.inner {
            Inner::Pipe(pipe) => pipe_drain(pipe),
            Inner::Tty(tty) => tty_drain(tty),
        }
    }

    /// Print the prompt, flush, then take one line from the shared source.
    ///
    /// Returns `None` on EOF (Ctrl-D). In cooked mode line editing, paste
    /// and input methods are provided by the terminal. The prompt goes to
    /// stderr so stdout carries only assistant text and stays clean under
    /// pipe redirection.
    pub async fn read_prompt_line(&mut self, prompt: &str) -> io::Result<Option<String>> {
        {
            let stderr = io::stderr();
            let mut err = stderr.lock();
            err.write_all(prompt.as_bytes())?;
            err.flush()?;
        }
        Ok(self.next_line().await)
    }
}

async fn pipe_next_line(pipe: &mut PipeReader) -> Option<String> {
    loop {
        if let Some(pos) = pipe.pending.iter().position(|&byte| byte == b'\n') {
            let mut line: Vec<u8> = pipe.pending.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Some(String::from_utf8_lossy(&line).into_owned());
        }
        if pipe.finished {
            return None;
        }
        let Some(rx) = pipe.rx.as_mut() else {
            // Receiver handed to the raw editor: EOF for this consumer,
            // but do not latch `finished` — the receiver comes back
            // when the prompt ends.
            return None;
        };
        match rx.recv().await {
            Some(chunk) => pipe.pending.extend_from_slice(&chunk),
            None => {
                pipe.finished = true;
                if pipe.pending.is_empty() {
                    return None;
                }
                let rest = std::mem::take(&mut pipe.pending);
                return Some(String::from_utf8_lossy(&rest).into_owned());
            }
        }
    }
}

fn pipe_drain(pipe: &mut PipeReader) -> usize {
    let mut dropped = pipe.pending.iter().filter(|&&b| b == b'\n').count();
    pipe.pending.clear();
    loop {
        let try_result = pipe.rx.as_mut().map(|rx| rx.try_recv());
        match try_result {
            Some(Ok(chunk)) => dropped += chunk.iter().filter(|&&b| b == b'\n').count(),
            Some(Err(mpsc::error::TryRecvError::Empty)) | None => return dropped,
            Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
                pipe.finished = true;
                return dropped;
            }
        }
    }
}

/// TTY cooked-line assembly from key events. The terminal stays in cooked
/// mode here so Ctrl-C still arrives as SIGINT (handled by the caller's
/// select); control and alt combinations are ignored for line purposes and
/// only plain characters, Backspace and Enter shape the line.
async fn tty_next_line(tty: &mut TtyReader) -> Option<String> {
    loop {
        if let Some(line) = tty.pending_lines.pop_front() {
            return Some(line);
        }
        if tty.finished {
            if tty.partial.is_empty() {
                return None;
            }
            return Some(std::mem::take(&mut tty.partial));
        }
        let stream = tty.stream.as_mut()?;
        match stream.next().await {
            Some(Ok(Event::Key(key))) => {
                if key.kind.is_release() {
                    continue;
                }
                use crossterm::event::{KeyCode as C, KeyModifiers as M};
                if key.modifiers.contains(M::CONTROL) || key.modifiers.contains(M::ALT) {
                    continue;
                }
                match key.code {
                    C::Enter => {
                        return Some(std::mem::take(&mut tty.partial));
                    }
                    C::Backspace => {
                        tty.partial.pop();
                    }
                    C::Char(c) => {
                        tty.partial.push(c);
                    }
                    _ => {}
                }
            }
            Some(Ok(Event::Mouse(mouse))) => {
                // No mouse capture in mini: buttons stay native and wheel
                // notches have no cooked-line target (history browsing
                // lives in the raw editor path). Named explicitly so a
                // future capture change lands here first.
                use crossterm::event::MouseEventKind;
                match mouse.kind {
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {}
                    _ => {}
                }
            }
            Some(Ok(Event::Paste(_))) | Some(Ok(Event::Resize(_, _))) | Some(Ok(_)) => {}
            Some(Err(_)) | None => {
                tty.finished = true;
                if tty.partial.is_empty() && tty.pending_lines.is_empty() {
                    return None;
                }
                if let Some(line) = tty.pending_lines.pop_front() {
                    return Some(line);
                }
                if !tty.partial.is_empty() {
                    return Some(std::mem::take(&mut tty.partial));
                }
                return None;
            }
        }
    }
}

/// Non-blocking TTY drain: queued lines plus any ready key events. The
/// event stream is idle while draining (no concurrent poll), so a zero
/// timeout poll plus read loop is safe.
fn tty_drain(tty: &mut TtyReader) -> usize {
    let mut dropped = tty.pending_lines.len();
    tty.pending_lines.clear();
    tty.partial.clear();
    if tty.stream.is_none() {
        return dropped;
    }
    let mut partial = String::new();
    while let Ok(true) = crossterm::event::poll(Duration::from_secs(0)) {
        match crossterm::event::read() {
            Ok(Event::Key(key)) => {
                if key.kind.is_release() {
                    continue;
                }
                use crossterm::event::{KeyCode as C, KeyModifiers as M};
                if key.modifiers.contains(M::CONTROL) || key.modifiers.contains(M::ALT) {
                    continue;
                }
                match key.code {
                    C::Enter => {
                        dropped += 1;
                        partial.clear();
                    }
                    C::Backspace => {
                        partial.pop();
                    }
                    C::Char(c) => {
                        partial.push(c);
                    }
                    _ => {}
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_known_commands() {
        assert_eq!(parse_command("/quit"), Some(SlashCommand::Quit));
        assert_eq!(parse_command("/exit"), Some(SlashCommand::Quit));
        assert_eq!(parse_command("/help"), Some(SlashCommand::Help));
        assert_eq!(parse_command("/clear"), Some(SlashCommand::Clear));
        assert_eq!(parse_command("/new"), Some(SlashCommand::New));
        assert_eq!(parse_command("/history"), Some(SlashCommand::History(None)));
        assert_eq!(
            parse_command("/history cargo"),
            Some(SlashCommand::History(Some("cargo".to_string())))
        );
        assert_eq!(parse_command("/history a b"), None);
        assert_eq!(parse_command("/rerun"), Some(SlashCommand::Rerun(None)));
        assert_eq!(
            parse_command("/rerun 3"),
            Some(SlashCommand::Rerun(Some(3)))
        );
        assert_eq!(parse_command("/rerun 0"), None);
        assert_eq!(parse_command("/rerun abc"), None);
        assert_eq!(parse_command("/rerun 1 2"), None);
        assert_eq!(parse_command("/quit now"), None);
        assert_eq!(parse_command("hello"), None);
        assert_eq!(parse_command("/unknown"), None);
    }

    #[test]
    fn parses_display_commands() {
        assert_eq!(
            parse_command("/markdown"),
            Some(SlashCommand::Markdown(None))
        );
        assert_eq!(
            parse_command("/markdown plain"),
            Some(SlashCommand::Markdown(Some("plain".to_string())))
        );
        assert_eq!(parse_command("/markdown a b"), None);
        assert_eq!(
            parse_command("/reasoning off"),
            Some(SlashCommand::Reasoning(Some("off".to_string())))
        );
        assert_eq!(parse_command("/usage"), Some(SlashCommand::Usage(None)));
        assert_eq!(
            parse_command("/tools on"),
            Some(SlashCommand::Tools(Some("on".to_string())))
        );
        assert_eq!(parse_command("/display"), Some(SlashCommand::Display));
        assert_eq!(parse_command("/display x"), None);
    }

    #[test]
    fn history_dedups_consecutive() {
        let mut history = History::new();
        history.push("a");
        history.push("a");
        assert_eq!(history.entries.len(), 1);
        history.push("b");
        assert_eq!(history.entries.len(), 2);
    }

    #[test]
    fn history_caps_at_limit() {
        let mut history = History::new();
        for index in 0..HISTORY_LIMIT + 10 {
            history.push(&format!("line {index}"));
        }
        assert_eq!(history.entries.len(), HISTORY_LIMIT);
    }

    fn test_reader() -> (LineReader, mpsc::UnboundedSender<Vec<u8>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (LineReader::from_receiver(rx), tx)
    }

    #[tokio::test]
    async fn reader_delivers_lines_in_order() {
        let (mut reader, tx) = test_reader();
        tx.send(b"first\n".to_vec()).unwrap();
        tx.send(b"second\n".to_vec()).unwrap();
        assert_eq!(reader.next_line().await.as_deref(), Some("first"));
        assert_eq!(reader.next_line().await.as_deref(), Some("second"));
    }

    #[tokio::test]
    async fn reader_reassembles_lines_split_across_chunks() {
        let (mut reader, tx) = test_reader();
        tx.send(b"hel".to_vec()).unwrap();
        tx.send("lo世\n".as_bytes().to_vec()).unwrap();
        assert_eq!(reader.next_line().await.as_deref(), Some("hello世"));
    }

    #[tokio::test]
    async fn reader_reports_closed_after_sender_drop_and_drain() {
        let (mut reader, tx) = test_reader();
        tx.send(b"late\n".to_vec()).unwrap();
        drop(tx);
        assert_eq!(reader.next_line().await.as_deref(), Some("late"));
        assert_eq!(reader.next_line().await, None);
        assert_eq!(reader.next_line().await, None);
    }

    #[tokio::test]
    async fn drain_drops_stale_lines_and_counts_them() {
        let (mut reader, tx) = test_reader();
        tx.send(b"stale-1\n".to_vec()).unwrap();
        tx.send(b"stale-2\n".to_vec()).unwrap();
        assert_eq!(reader.drain(), 2);
        assert_eq!(reader.drain(), 0);
        tx.send(b"fresh\n".to_vec()).unwrap();
        assert_eq!(reader.next_line().await.as_deref(), Some("fresh"));
    }

    #[tokio::test]
    async fn drain_on_closed_channel_marks_finished() {
        let (mut reader, tx) = test_reader();
        tx.send(b"x\n".to_vec()).unwrap();
        drop(tx);
        assert_eq!(reader.drain(), 1);
        assert_eq!(reader.next_line().await, None);
    }

    #[tokio::test]
    async fn eof_yields_pending_unterminated_line() {
        let (mut reader, tx) = test_reader();
        tx.send(b"no trailing newline".to_vec()).unwrap();
        drop(tx);
        assert_eq!(
            reader.next_line().await.as_deref(),
            Some("no trailing newline")
        );
        assert_eq!(reader.next_line().await, None);
    }

    #[tokio::test]
    async fn stream_borrow_reports_eof_while_borrowed() {
        let mut reader = LineReader::for_tty_test();
        assert!(reader.is_tty());
        assert!(reader.next_line().await.is_none(), "no stream: EOF");
        assert_eq!(reader.drain(), 0);
    }

    #[test]
    fn history_suggest_prefers_the_newest_prefix_match() {
        let mut history = History::new();
        history.push("git status");
        history.push("cargo test");
        history.push("cargo nextest run");
        assert_eq!(history.suggest("cargo "), Some("nextest run".to_string()));
        assert_eq!(history.suggest("git "), Some("status".to_string()));
        assert_eq!(history.suggest(""), None);
        assert_eq!(history.suggest("cargo nextest run"), None);
        assert_eq!(history.suggest("make"), None);
    }

    #[test]
    fn history_filter_lists_prefix_matches_oldest_first() {
        let mut history = History::new();
        history.push("cargo test");
        history.push("git status");
        history.push("cargo fmt");
        assert_eq!(
            history.filter_prefix("cargo"),
            vec![(1, "cargo test"), (3, "cargo fmt")]
        );
        assert!(history.filter_prefix("make").is_empty());
    }

    #[test]
    fn history_lookup_is_one_based() {
        let mut history = History::new();
        history.push("first");
        history.push("second");
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(1), Some("first"));
        assert_eq!(history.get(2), Some("second"));
        assert_eq!(history.get(0), None);
        assert_eq!(history.get(3), None);
        let recent = history.recent(10);
        assert_eq!(recent, vec![(1, "first"), (2, "second")]);
        let tail = history.recent(1);
        assert_eq!(tail, vec![(2, "second")]);
    }

    #[test]
    fn history_skips_blank_and_flattens_newlines() {
        let mut history = History::new();
        history.push("   ");
        assert!(history.is_empty());
        history.push("a\nb");
        assert_eq!(history.get(1), Some("a b"));
    }

    #[test]
    fn history_snapshot_is_oldest_first() {
        let mut history = History::new();
        history.push("first");
        history.push("second");
        assert_eq!(history.snapshot(), vec!["first", "second"]);
    }

    #[test]
    fn history_round_trips_through_a_file() {
        let dir = std::env::temp_dir().join(format!("wf-mini-history-{}", std::process::id()));
        let path = dir.join("history");
        let _ = std::fs::remove_dir_all(&dir);
        let mut history = History::load(path.clone());
        history.push("first");
        history.push("first");
        history.push("second");
        assert!(path.exists());
        let reloaded = History::load(path.clone());
        assert_eq!(reloaded.len(), 2);
        assert_eq!(reloaded.get(1), Some("first"));
        assert_eq!(reloaded.get(2), Some("second"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_load_missing_file_is_empty() {
        let path = std::env::temp_dir().join(format!("wf-mini-absent-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let history = History::load(path);
        assert!(history.is_empty());
    }

    #[test]
    fn resolve_history_path_prefers_flag_then_env_then_default() {
        let custom = std::path::PathBuf::from("/tmp/custom-history");
        assert_eq!(
            resolve_history_path(Some(custom.as_path()), false),
            Some(custom.clone())
        );
        assert_eq!(resolve_history_path(Some(custom.as_path()), true), None);
        assert_eq!(resolve_history_path(None, true), None);
    }

    #[test]
    fn tty_reader_reports_tty_and_drains_empty() {
        let mut reader = LineReader::for_tty_test();
        assert!(reader.is_tty());
        assert!(reader.take_event_stream().is_none());
        assert_eq!(reader.drain(), 0);
    }

    #[test]
    fn tty_drain_empty_is_zero_and_pipe_is_not_tty() {
        let mut tty = LineReader::for_tty_test();
        assert_eq!(tty.drain(), 0);
        let (tx, rx) = mpsc::unbounded_channel();
        drop(tx);
        let pipe = LineReader::from_receiver(rx);
        assert!(!pipe.is_tty());
    }
}
