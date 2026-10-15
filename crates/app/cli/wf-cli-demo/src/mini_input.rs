//! Mini prompt-layer primitives for demos.
//!
//! Re-exports the real `wf-mini` prompt layer — slash commands, history,
//! the partial-accept chunk length and the approval answers — so demos
//! exercise the actual implementation instead of a mirrored copy. Only
//! demo-specific content (the seeded history) lives here.

pub use wf_mini::approval::{approval_prompt, is_affirmative};
pub use wf_mini::editor::partial_word_bytes;
pub use wf_mini::input::{parse_command, History, SlashCommand};

/// Seed history shared by the input demos.
pub const HISTORY_SEED: &[&str] = &["cargo test", "cargo nextest run", "git status"];

/// History pre-loaded with [`HISTORY_SEED`].
pub fn seeded_history() -> History {
    let mut history = History::new();
    for entry in HISTORY_SEED {
        history.push(entry);
    }
    history
}
