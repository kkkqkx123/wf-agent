//! Mini prompt-layer primitives for demos: slash commands, history,
//! suggestion ghost text, and approval answers.
//!
//! The prompt lives on stderr with a dim ghost suggestion. Slash input maps
//! to session actions; anything else is a prompt. History backs Up/Down
//! browsing, `/history`, `/rerun`, and the inline prediction.

use std::collections::VecDeque;

/// Slash commands accepted at the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slash {
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
pub fn parse_slash(line: &str) -> Option<Slash> {
    let mut tokens = line.split_whitespace();
    match tokens.next()? {
        "/quit" | "/exit" | ":q" if tokens.next().is_none() => Some(Slash::Quit),
        "/help" | "/?" if tokens.next().is_none() => Some(Slash::Help),
        "/clear" if tokens.next().is_none() => Some(Slash::Clear),
        "/new" if tokens.next().is_none() => Some(Slash::New),
        "/history" => match tokens.next() {
            None => Some(Slash::History(None)),
            Some(prefix) if tokens.next().is_none() => {
                Some(Slash::History(Some(prefix.to_string())))
            }
            _ => None,
        },
        "/rerun" => match tokens.next() {
            None => Some(Slash::Rerun(None)),
            Some(number) => match number.parse::<usize>() {
                Ok(n) if n >= 1 && tokens.next().is_none() => Some(Slash::Rerun(Some(n))),
                _ => None,
            },
        },
        "/markdown" => match tokens.next() {
            None => Some(Slash::Markdown(None)),
            Some(mode) if tokens.next().is_none() => Some(Slash::Markdown(Some(mode.to_string()))),
            _ => None,
        },
        "/reasoning" => match tokens.next() {
            None => Some(Slash::Reasoning(None)),
            Some(toggle) if tokens.next().is_none() => {
                Some(Slash::Reasoning(Some(toggle.to_string())))
            }
            _ => None,
        },
        "/usage" => match tokens.next() {
            None => Some(Slash::Usage(None)),
            Some(toggle) if tokens.next().is_none() => Some(Slash::Usage(Some(toggle.to_string()))),
            _ => None,
        },
        "/tools" => match tokens.next() {
            None => Some(Slash::Tools(None)),
            Some(toggle) if tokens.next().is_none() => Some(Slash::Tools(Some(toggle.to_string()))),
            _ => None,
        },
        "/display" if tokens.next().is_none() => Some(Slash::Display),
        _ => None,
    }
}

/// Seed history shared by the input demos.
pub const HISTORY_SEED: &[&str] = &["cargo test", "cargo nextest run", "git status"];

/// In-memory prompt history with consecutive-duplicate suppression.
#[derive(Debug, Default)]
pub struct PreviewHistory {
    entries: VecDeque<String>,
}

impl PreviewHistory {
    pub fn seeded() -> Self {
        let mut history = Self::default();
        for entry in HISTORY_SEED {
            history.push(entry);
        }
        history
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
        self.entries.push_back(trimmed.to_string());
        while self.entries.len() > 200 {
            self.entries.pop_front();
        }
    }

    /// 1-based lookup for `/rerun n`.
    pub fn get(&self, number: usize) -> Option<&str> {
        self.entries.get(number.checked_sub(1)?).map(String::as_str)
    }

    /// Last `count` entries with 1-based numbers, oldest first.
    pub fn recent(&self, count: usize) -> Vec<(usize, &str)> {
        let skip = self.entries.len().saturating_sub(count);
        self.entries
            .iter()
            .skip(skip)
            .enumerate()
            .map(|(offset, entry)| (skip + offset + 1, entry.as_str()))
            .collect()
    }

    /// Every entry starting with `prefix`, oldest first.
    pub fn filter_prefix(&self, prefix: &str) -> Vec<(usize, &str)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.starts_with(prefix))
            .map(|(offset, entry)| (offset + 1, entry.as_str()))
            .collect()
    }

    /// Inline-prediction suffix: rest of the newest longer entry.
    pub fn suggest(&self, prefix: &str) -> Option<String> {
        if prefix.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.starts_with(prefix) && entry.len() > prefix.len())
            .map(|entry| entry[prefix.len()..].to_string())
    }
}

/// Byte length of the next partial-accept chunk of a ghost suffix.
pub fn partial_word_bytes(suffix: &str) -> usize {
    let chars: Vec<(usize, char)> = suffix.char_indices().collect();
    if chars.is_empty() {
        return suffix.len();
    }
    let mut i = 0;
    if chars[0].1.is_whitespace() {
        while i < chars.len() && chars[i].1.is_whitespace() {
            i += 1;
        }
    } else {
        while i < chars.len() && !chars[i].1.is_whitespace() {
            i += 1;
        }
        while i < chars.len() && chars[i].1.is_whitespace() {
            i += 1;
        }
    }
    chars.get(i).map(|(byte, _)| *byte).unwrap_or(suffix.len())
}

/// Accept common affirmative approval answers; everything else denies.
pub fn is_affirmative(answer: &str) -> bool {
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Approval prompt text for one tool.
pub fn approval_prompt(tool_name: &str, risk: Option<&str>, timeout_secs: u64) -> String {
    match risk {
        Some(level) => format!(
            "Allow tool '{tool_name}' [{level}]? [y/N] (default N, {timeout_secs}s timeout)"
        ),
        None => format!("Allow tool '{tool_name}'? [y/N] (default N, {timeout_secs}s timeout)"),
    }
}
