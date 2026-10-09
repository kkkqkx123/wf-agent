pub mod content;
pub mod diff;
pub mod line_diff;
pub mod word_diff;

use serde::{Deserialize, Serialize};

/// Line level instance key identifying one row level diff participant.
/// This key is unrelated to execution actor identity and lives outside the
/// diff engine so the two identity types cannot be confused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LineInstanceId(pub String);

impl std::fmt::Display for LineInstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for LineInstanceId {
    fn from(s: &str) -> Self {
        LineInstanceId(s.to_string())
    }
}

impl From<String> for LineInstanceId {
    fn from(s: String) -> Self {
        LineInstanceId(s)
    }
}

pub use diff::{
    content_hash, diff_stats_for_text, inline_word_diff, is_binary, unified_diff_text, DiffStats,
};
pub use line_diff::{
    diff_stat_counts, diff_to_line_diff, format_unified_diff, should_use_full_snapshot,
    should_use_full_snapshot_content, DiffOp, Hunk, LineDiff, DEFAULT_FULL_SNAPSHOT_THRESHOLD,
};
pub use word_diff::{diff_words, WordChange, WordDiff};
