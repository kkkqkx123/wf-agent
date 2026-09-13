//! Line-discipline input: blocking prompt read, slash commands, history.

use std::collections::VecDeque;
use std::io::{self, Write};

/// Maximum history entries retained in memory.
const HISTORY_LIMIT: usize = 200;

/// Slash commands accepted at the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Quit,
    Help,
    Clear,
}

/// Parse a trimmed input line into a slash command, if it is one.
pub fn parse_command(line: &str) -> Option<SlashCommand> {
    match line {
        "/quit" | "/exit" | ":q" => Some(SlashCommand::Quit),
        "/help" | "/?" => Some(SlashCommand::Help),
        "/clear" => Some(SlashCommand::Clear),
        _ => None,
    }
}

/// In-memory prompt history with consecutive-duplicate suppression.
#[derive(Debug, Default)]
pub struct History {
    entries: VecDeque<String>,
}

impl History {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    pub fn push(&mut self, line: &str) {
        if self.entries.back().map(String::as_str) == Some(line) {
            return;
        }
        self.entries.push_back(line.to_string());
        if self.entries.len() > HISTORY_LIMIT {
            self.entries.pop_front();
        }
    }
}

/// Print the prompt, flush, then read one line from stdin.
///
/// Returns `None` on EOF (Ctrl-D). The terminal stays in cooked mode, so
/// line editing, paste and input methods are provided by the terminal.
/// The prompt goes to stderr so stdout carries only assistant text and
/// stays clean under pipe redirection.
pub async fn read_prompt_line(prompt: &str) -> io::Result<Option<String>> {
    {
        let stderr = io::stderr();
        let mut err = stderr.lock();
        err.write_all(prompt.as_bytes())?;
        err.flush()?;
    }
    let mut line = String::new();
    let n = tokio::io::AsyncBufReadExt::read_line(
        &mut tokio::io::BufReader::new(tokio::io::stdin()),
        &mut line,
    )
    .await?;
    if n == 0 {
        return Ok(None);
    }
    Ok(Some(line))
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
}
