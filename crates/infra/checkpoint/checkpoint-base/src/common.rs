pub mod content;
pub mod diff;

pub use diff::{
    content_hash, diff_stats_for_text, inline_word_diff, is_binary, unified_diff_text, DiffStats,
};
