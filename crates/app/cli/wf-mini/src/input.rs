//! Line-discipline input: single stdin pump, slash commands, history.

use std::collections::VecDeque;
use std::io::{self, Write};

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
    History,
    Rerun(Option<usize>),
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
        "/history" if tokens.next().is_none() => Some(SlashCommand::History),
        "/rerun" => match tokens.next() {
            None => Some(SlashCommand::Rerun(None)),
            Some(number) => match number.parse::<usize>() {
                Ok(n) if n >= 1 && tokens.next().is_none() => Some(SlashCommand::Rerun(Some(n))),
                _ => None,
            },
        },
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
    matches!(
        std::env::var(name)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
}

/// Process-wide single owner of stdin line reads.
///
/// One pump task is the only place that touches stdin; the prompt loop and
/// the approval handler both take lines from the same channel, so a late
/// approval answer or a line typed mid-stream can never be misread as the
/// next prompt: the session drains unconsumed lines whenever consumption
/// passes back to the prompt. A half-typed line without a newline needs no
/// drain: in cooked mode Ctrl-C makes the terminal discard it.
///
/// The channel is unbounded so the pump never blocks: backpressure here
/// would stall interrupt handling while buying nothing (a line is tiny).
/// EOF closes the channel; once drained, the reader reports closed.
pub struct LineReader {
    rx: mpsc::UnboundedReceiver<String>,
    finished: bool,
    _pump: Option<tokio::task::JoinHandle<()>>,
}

impl LineReader {
    /// Start the stdin pump. Call once per process; the session owns it.
    pub fn for_stdin() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let pump = tokio::spawn(async move {
            let stdin = tokio::io::stdin();
            let mut buffered = tokio::io::BufReader::new(stdin);
            loop {
                let mut line = String::new();
                match tokio::io::AsyncBufReadExt::read_line(&mut buffered, &mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            rx,
            finished: false,
            _pump: Some(pump),
        }
    }

    /// Wrap an existing channel. Tests use this to feed fake input without
    /// touching real stdin.
    #[cfg(test)]
    pub fn from_receiver(rx: mpsc::UnboundedReceiver<String>) -> Self {
        Self {
            rx,
            finished: false,
            _pump: None,
        }
    }

    /// Wait for the next line. Returns `None` once stdin hit EOF and every
    /// buffered line was consumed.
    pub async fn next_line(&mut self) -> Option<String> {
        if self.finished {
            return None;
        }
        match self.rx.recv().await {
            Some(line) => Some(line),
            None => {
                self.finished = true;
                None
            }
        }
    }

    /// Drop every buffered but unconsumed line, returning how many were
    /// dropped. Never blocks.
    pub fn drain(&mut self) -> usize {
        let mut dropped = 0;
        loop {
            match self.rx.try_recv() {
                Ok(_) => dropped += 1,
                Err(mpsc::error::TryRecvError::Empty) => return dropped,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.finished = true;
                    return dropped;
                }
            }
        }
    }

    /// Print the prompt, flush, then take one line from the shared channel.
    ///
    /// Returns `None` on EOF (Ctrl-D). The terminal stays in cooked mode,
    /// so line editing, paste and input methods are provided by the
    /// terminal. The prompt goes to stderr so stdout carries only assistant
    /// text and stays clean under pipe redirection.
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

impl Drop for LineReader {
    fn drop(&mut self) {
        if let Some(pump) = self._pump.take() {
            pump.abort();
        }
    }
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
        assert_eq!(parse_command("/history"), Some(SlashCommand::History));
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

    fn test_reader() -> (LineReader, mpsc::UnboundedSender<String>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (LineReader::from_receiver(rx), tx)
    }

    #[tokio::test]
    async fn reader_delivers_lines_in_order() {
        let (mut reader, tx) = test_reader();
        tx.send("first\n".to_string()).unwrap();
        tx.send("second\n".to_string()).unwrap();
        assert_eq!(reader.next_line().await.as_deref(), Some("first\n"));
        assert_eq!(reader.next_line().await.as_deref(), Some("second\n"));
    }

    #[tokio::test]
    async fn reader_reports_closed_after_sender_drop_and_drain() {
        let (mut reader, tx) = test_reader();
        tx.send("late\n".to_string()).unwrap();
        drop(tx);
        assert_eq!(reader.next_line().await.as_deref(), Some("late\n"));
        assert_eq!(reader.next_line().await, None);
        assert_eq!(reader.next_line().await, None);
    }

    #[tokio::test]
    async fn drain_drops_stale_lines_and_counts_them() {
        let (mut reader, tx) = test_reader();
        tx.send("stale-1\n".to_string()).unwrap();
        tx.send("stale-2\n".to_string()).unwrap();
        assert_eq!(reader.drain(), 2);
        assert_eq!(reader.drain(), 0);
        tx.send("fresh\n".to_string()).unwrap();
        assert_eq!(reader.next_line().await.as_deref(), Some("fresh\n"));
    }

    #[tokio::test]
    async fn drain_on_closed_channel_marks_finished() {
        let (mut reader, tx) = test_reader();
        tx.send("x\n".to_string()).unwrap();
        drop(tx);
        assert_eq!(reader.drain(), 1);
        assert_eq!(reader.next_line().await, None);
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
}
