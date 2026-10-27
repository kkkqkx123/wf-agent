//! Suggestion engine: pluggable strategies over one buffer prefix.
//!
//! Modeled on zsh-autosuggestions' fetch layer (`src/fetch.zsh`): strategies
//! are tried in registration order and the first one to produce a result
//! wins; every result is validated as a strict prefix extension of the
//! current buffer before it is shown (`fetch.zsh:22-23`).
//!
//! Built-in strategies, in default order:
//! - [`HistoryStrategy`] — newest history entry matching the prefix
//!   (`strategies/history.zsh`), plus ignore-space semantics: a buffer that
//!   starts with a space is never suggested (shell `HIST_IGNORE_SPACE`
//!   convention for intentionally unrecorded input).
//! - [`MatchPrevCmdStrategy`] — like `strategies/match_prev_cmd.zsh`: the
//!   matched entry must directly follow the previous prompt in history, so
//!   suggestions reflect actual command sequences, not coincidental
//!   prefixes. wf-mini reads the previous prompt from the session instead
//!   of shell history.
//! - [`CommandStrategy`] — slash command name completion with
//!   longest-common-prefix resolution for ambiguous prefixes.
//!
//! [`SuggestBroker`] carries the cancel-pending semantics from
//! `async.zsh:12-27`: every fetch opens a token, and a late result is
//! discarded unless its token is still current. The editor bumps the token
//! on every buffer modification, so a keystroke always invalidates the
//! fetch before it. Today's strategies are in-memory queries evaluated
//! synchronously inside the token window (the fast path); when a slow or
//! off-process strategy lands, it can compute under the same token and let
//! [`SuggestBroker::is_current`] decide whether the result is still wanted.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Read-only context a strategy may consult for one fetch.
pub struct SuggestCtx<'a> {
    /// History entries, oldest first.
    pub history: &'a [String],
    /// The previously submitted prompt, if any (backs `match_prev_cmd`).
    pub prev_prompt: Option<&'a str>,
    /// Skill names available for mention completion.
    pub skills: &'a [String],
    /// Workflow ids available for mention completion.
    pub workflows: &'a [String],
    /// Project root used for file mention scanning.
    pub project_root: Option<&'a std::path::Path>,
}

/// One suggestion source. Returns the FULL suggested line — callers
/// re-validate the prefix relation and derive the display suffix.
pub trait SuggestStrategy: Send + Sync {
    /// Strategy identifier, kept for diagnostics on future strategies.
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    fn suggest(&self, buffer: &str, ctx: &SuggestCtx) -> Option<String>;
}

/// Newest history entry matching the prefix (zsh `history` strategy).
pub struct HistoryStrategy;

impl SuggestStrategy for HistoryStrategy {
    fn name(&self) -> &'static str {
        "history"
    }

    fn suggest(&self, buffer: &str, ctx: &SuggestCtx) -> Option<String> {
        if buffer.is_empty() || buffer.starts_with(' ') {
            return None;
        }
        ctx.history
            .iter()
            .rev()
            .find(|entry| entry.starts_with(buffer) && entry.len() > buffer.len())
            .cloned()
    }
}

/// History entry matching the prefix whose predecessor in history is the
/// previous prompt (zsh `match_prev_cmd` strategy).
pub struct MatchPrevCmdStrategy;

impl SuggestStrategy for MatchPrevCmdStrategy {
    fn name(&self) -> &'static str {
        "match_prev_cmd"
    }

    fn suggest(&self, buffer: &str, ctx: &SuggestCtx) -> Option<String> {
        let prev = ctx.prev_prompt?;
        if buffer.is_empty() || buffer.starts_with(' ') {
            return None;
        }
        let history = ctx.history;
        // Newest first; `history[i - 1]` is the entry right before `i`.
        for i in (1..history.len()).rev() {
            if history[i].starts_with(buffer)
                && history[i].len() > buffer.len()
                && history[i - 1] == prev
            {
                return Some(history[i].clone());
            }
        }
        None
    }
}

/// Slash command name completion with longest-common-prefix resolution.
pub struct CommandStrategy;

/// Alias groups; all names in a group complete to each other.
const COMMANDS: &[&[&str]] = &[
    &["/quit", "/exit", ":q"],
    &["/help", "/?"],
    &["/clear"],
    &["/new"],
    &["/history"],
    &["/rerun"],
    &["/markdown"],
    &["/reasoning"],
    &["/usage"],
    &["/tools"],
    &["/display"],
    &["/recap"],
];

impl SuggestStrategy for CommandStrategy {
    fn name(&self) -> &'static str {
        "command"
    }

    fn suggest(&self, buffer: &str, _ctx: &SuggestCtx) -> Option<String> {
        if !buffer.starts_with('/') || buffer.chars().any(char::is_whitespace) {
            return None;
        }
        let mut candidates: Vec<&str> = COMMANDS
            .iter()
            .flat_map(|group| group.iter().copied())
            .filter(|name| name.starts_with(buffer))
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        match candidates.as_slice() {
            // A single candidate completes to the full name when longer.
            [only] => (only.len() > buffer.len()).then(|| (*only).to_string()),
            // Multiple candidates complete to their common prefix; when the
            // buffer already is that prefix the outcome is ambiguous and
            // nothing is suggested.
            _ => {
                let prefix = longest_common_prefix(&candidates)?;
                (prefix.len() > buffer.len()).then(|| prefix.to_string())
            }
        }
    }
}

/// File mention completion: turns `@` / `@file:` prefixes into a concrete
/// project-relative path using a bounded filesystem scan, mirroring the
/// `@file` data source used by the TUI composer.
pub struct MentionStrategy;

impl SuggestStrategy for MentionStrategy {
    fn name(&self) -> &'static str {
        "mention"
    }

    fn suggest(&self, buffer: &str, ctx: &SuggestCtx) -> Option<String> {
        if buffer.is_empty() {
            return None;
        }
        let at = buffer.rfind('@')?;
        let after = &buffer[at + 1..];
        if after.contains(' ')
            || after.contains('\t')
            || after.contains('\n')
            || after.contains('@')
        {
            return None;
        }
        let (kind, term) = match after.find(':') {
            Some(pos) => (&after[..pos], &after[pos + 1..]),
            None => ("", after),
        };
        let root = ctx.project_root.unwrap_or_else(|| Path::new("."));
        let term_lower = term.to_ascii_lowercase();
        let mut candidates: Vec<String> = Vec::new();

        if kind.is_empty() || kind == "file" {
            for path in scan_files(root, None) {
                if term.is_empty() || path.to_ascii_lowercase().contains(&term_lower) {
                    candidates.push(if kind.is_empty() {
                        format!("@{path}")
                    } else {
                        format!("@file:{path}")
                    });
                }
            }
        }
        if kind.is_empty() || kind == "skill" {
            for skill in ctx.skills {
                if term.is_empty() || skill.to_ascii_lowercase().contains(&term_lower) {
                    candidates.push(format!("@skill:{skill}"));
                }
            }
        }
        if kind.is_empty() || kind == "workflow" || kind == "wf" {
            for workflow in ctx.workflows {
                if term.is_empty() || workflow.to_ascii_lowercase().contains(&term_lower) {
                    candidates.push(format!("@workflow:{workflow}"));
                }
            }
        }

        candidates.sort();
        candidates.dedup();
        candidates
            .into_iter()
            .find(|candidate| candidate.starts_with(buffer) && candidate.len() > buffer.len())
    }
}

fn scan_files(root: &Path, _query: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    let limit = 500usize;
    let max_depth = 6usize;
    let mut stack: Vec<(std::path::PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > max_depth || out.len() >= limit {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel) = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
            else {
                continue;
            };
            if rel
                .split('/')
                .any(|part| matches!(part, ".git" | "node_modules" | "target" | ".next" | "dist"))
            {
                continue;
            }
            if path.is_dir() {
                stack.push((path, depth + 1));
            } else if path.is_file() {
                out.push(rel);
                if out.len() >= limit {
                    break;
                }
            }
        }
    }
    out.sort();
    out
}

fn longest_common_prefix(candidates: &[&str]) -> Option<String> {
    let mut prefix: Option<String> = None;
    for candidate in candidates {
        match &prefix {
            None => prefix = Some((*candidate).to_string()),
            Some(current) => {
                let common = current
                    .char_indices()
                    .zip(candidate.chars())
                    .take_while(|((_, a), b)| a == b)
                    .count();
                let kept: String = current.chars().take(common).collect();
                if kept.is_empty() {
                    return None;
                }
                prefix = Some(kept);
            }
        }
    }
    prefix
}

/// Cancel-pending guard for suggestion fetches, ported from
/// zsh-autosuggestions' async request lifecycle (`async.zsh:12-27`): a new
/// keystroke invalidates every fetch opened before it, and a late result is
/// dropped unless its token is still current.
pub struct SuggestBroker {
    epoch: AtomicU64,
}

impl Default for SuggestBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl SuggestBroker {
    pub fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
        }
    }

    /// Open a fetch window; every later `begin` invalidates it.
    pub fn begin(&self) -> u64 {
        self.epoch.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Whether a fetch opened with `token` is still the latest one.
    pub fn is_current(&self, token: u64) -> bool {
        self.epoch.load(Ordering::Relaxed) == token
    }
}

/// Strategy pipeline with first-wins resolution and unified validation.
pub struct SuggestEngine {
    strategies: Vec<Box<dyn SuggestStrategy>>,
}

impl Default for SuggestEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SuggestEngine {
    pub fn new() -> Self {
        Self {
            strategies: vec![
                Box::new(HistoryStrategy),
                Box::new(MatchPrevCmdStrategy),
                Box::new(MentionStrategy),
                Box::new(CommandStrategy),
            ],
        }
    }

    /// First strategy to produce a strictly-longer prefix extension wins.
    pub fn suggest(&self, buffer: &str, ctx: &SuggestCtx) -> Option<String> {
        for strategy in &self.strategies {
            if let Some(suggestion) = strategy.suggest(buffer, ctx) {
                if suggestion.starts_with(buffer) && suggestion.len() > buffer.len() {
                    return Some(suggestion);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_ctx<'a>(history: &'a [String], prev: Option<&'a str>) -> SuggestCtx<'a> {
        SuggestCtx {
            history,
            prev_prompt: prev,
            skills: &[],
            workflows: &[],
            project_root: None,
        }
    }

    fn hist(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn history_strategy_prefers_the_newest_prefix_match() {
        let history = hist(&["git status", "cargo test", "cargo nextest run"]);
        let ctx = mk_ctx(&history, None);
        assert_eq!(
            HistoryStrategy.suggest("cargo ", &ctx),
            Some("cargo nextest run".to_string())
        );
        assert_eq!(HistoryStrategy.suggest("make", &ctx), None);
    }

    #[test]
    fn history_strategy_ignores_empty_and_space_prefixed_buffers() {
        let history = hist(&["cargo test"]);
        let ctx = mk_ctx(&history, None);
        assert_eq!(HistoryStrategy.suggest("", &ctx), None);
        assert_eq!(HistoryStrategy.suggest(" cargo", &ctx), None);
    }

    #[test]
    fn history_strategy_rejects_exact_repeats() {
        let history = hist(&["cargo test"]);
        let ctx = mk_ctx(&history, None);
        assert_eq!(HistoryStrategy.suggest("cargo test", &ctx), None);
    }

    #[test]
    fn match_prev_cmd_requires_matching_predecessor() {
        let history = hist(&["cargo test", "cargo nextest run", "git status", "git push"]);
        // "cargo nextest run" follows "cargo test" in history → suggested.
        let context = mk_ctx(&history, Some("cargo test"));
        assert_eq!(
            MatchPrevCmdStrategy.suggest("cargo ", &context),
            Some("cargo nextest run".to_string())
        );
        // "git push" follows "git status", not "cargo test" → rejected.
        assert_eq!(MatchPrevCmdStrategy.suggest("git ", &context), None);
        // With "git status" as the previous prompt, "git push" matches.
        let context = mk_ctx(&history, Some("git status"));
        assert_eq!(
            MatchPrevCmdStrategy.suggest("git ", &context),
            Some("git push".to_string())
        );
        assert_eq!(MatchPrevCmdStrategy.suggest("cargo ", &context), None);
    }

    #[test]
    fn match_prev_cmd_without_previous_prompt_is_silent() {
        let history = hist(&["cargo test", "cargo nextest run"]);
        assert_eq!(
            MatchPrevCmdStrategy.suggest("cargo", &mk_ctx(&history, None)),
            None
        );
    }

    #[test]
    fn command_strategy_completes_single_candidates() {
        let history = Vec::new();
        let ctx = mk_ctx(&history, None);
        assert_eq!(
            CommandStrategy.suggest("/hi", &ctx),
            Some("/history".to_string())
        );
        assert_eq!(
            CommandStrategy.suggest("/ex", &ctx),
            Some("/exit".to_string())
        );
    }

    #[test]
    fn command_strategy_resolves_ambiguity_to_common_prefix() {
        let history = Vec::new();
        let ctx = mk_ctx(&history, None);
        // /reasoning, /rerun share "/re".
        assert_eq!(CommandStrategy.suggest("/r", &ctx), Some("/re".to_string()));
        // Buffer already at the ambiguity point: nothing to add.
        assert_eq!(CommandStrategy.suggest("/re", &ctx), None);
        // Slash alone is ambiguous over the whole set.
        assert_eq!(CommandStrategy.suggest("/", &ctx), None);
    }

    #[test]
    fn command_strategy_ignores_non_commands_and_args() {
        let history = Vec::new();
        let ctx = mk_ctx(&history, None);
        assert_eq!(CommandStrategy.suggest("hi", &ctx), None);
        assert_eq!(CommandStrategy.suggest("/help me", &ctx), None);
        assert_eq!(CommandStrategy.suggest("/history", &ctx), None);
    }

    #[test]
    fn engine_first_wins_with_prefix_validation() {
        let engine = SuggestEngine::new();
        let history = hist(&["/help me carry this", "/history"]);
        let ctx = mk_ctx(&history, None);
        // History wins for a prefix that is not a command.
        assert_eq!(
            engine.suggest("/help me", &ctx),
            Some("/help me carry this".to_string())
        );
        // Command completion wins when history has nothing.
        let history = hist(&[]);
        let ctx = mk_ctx(&history, None);
        assert_eq!(engine.suggest("/hi", &ctx), Some("/history".to_string()));
    }

    #[test]
    fn broker_invalidates_stale_tokens() {
        let broker = SuggestBroker::new();
        let first = broker.begin();
        assert!(broker.is_current(first));
        let second = broker.begin();
        assert!(broker.is_current(second));
        assert!(!broker.is_current(first), "first fetch is stale now");
    }

    #[test]
    fn common_prefix_handles_unicode_boundaries() {
        assert_eq!(
            longest_common_prefix(&["/日本語です", "/日本語ね"]).as_deref(),
            Some("/日本語")
        );
        // No common extension: the shared root is still a prefix.
        assert_eq!(longest_common_prefix(&["/a", "/b"]).as_deref(), Some("/"));
        assert_eq!(longest_common_prefix(&["/a", "b"]), None);
    }
}
