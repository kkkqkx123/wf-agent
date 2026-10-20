//! In-memory prompt history with optional file persistence.

use std::collections::VecDeque;

use wf_cli_shared::app_config::MiniConfig;

/// Maximum history entries retained in memory.
pub(crate) const HISTORY_LIMIT: usize = 200;

/// In-memory prompt history with consecutive-duplicate suppression,
/// optionally backed by a plain-text file (one prompt per line, shell
/// convention) so `/history` and `/rerun` span processes.
///
/// File writes are best-effort and immediate (every submit appends, so a
/// killed process loses nothing); failures never surface to the session.
#[derive(Debug)]
pub struct History {
    entries: VecDeque<String>,
    file: Option<std::path::PathBuf>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            file: None,
            limit: HISTORY_LIMIT,
        }
    }
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty history with an explicit retention cap.
    pub fn with_limit(limit: usize) -> Self {
        let limit = if limit == 0 { HISTORY_LIMIT } else { limit };
        Self {
            entries: VecDeque::new(),
            file: None,
            limit,
        }
    }

    /// Load the file tail (up to the memory cap) and keep appending to it.
    /// A missing or unreadable file reads as empty history.
    pub fn load(path: std::path::PathBuf) -> Self {
        Self::load_with_limit(path, HISTORY_LIMIT)
    }

    /// Load with an explicit retention cap. A cap of zero falls back to
    /// [`HISTORY_LIMIT`].
    pub fn load_with_limit(path: std::path::PathBuf, limit: usize) -> Self {
        let limit = if limit == 0 { HISTORY_LIMIT } else { limit };
        let mut history = Self {
            entries: VecDeque::new(),
            file: Some(path.clone()),
            limit,
        };
        if let Ok(text) = std::fs::read_to_string(&path) {
            let lines: Vec<&str> = text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect();
            let tail = lines.len().saturating_sub(limit);
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
        if self.entries.len() > self.limit {
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

/// Resolve the history retention cap: env, config file, then default.
pub fn resolve_history_limit(file: Option<&MiniConfig>) -> usize {
    const MIN: usize = 1;
    const MAX: usize = 100_000;
    if let Ok(raw) = std::env::var("WF_MINI_HISTORY_LIMIT") {
        if let Ok(parsed) = raw.trim().parse::<usize>() {
            return parsed.clamp(MIN, MAX);
        }
    }
    file.and_then(|config| config.history_limit)
        .map(|value| value.clamp(MIN, MAX))
        .unwrap_or(HISTORY_LIMIT)
}

/// Load the mini config file, if present.
pub fn load_file_config() -> Option<MiniConfig> {
    let path = wf_cli_shared::app_config::config_file_path()?;
    wf_cli_shared::app_config::ConfigManager::load(&path)
        .ok()
        .map(|manager| manager.config().mini.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
