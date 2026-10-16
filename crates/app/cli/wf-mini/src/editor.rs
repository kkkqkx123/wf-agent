//! Raw-mode prompt editor: multiline-aware editing with ghost-text suggestions.
//!
//! The editor owns the crossterm event stream for the duration of one
//! prompt, mirrors the zsh-autosuggestions interaction model on a plain
//! terminal, and hands the stream back afterwards:
//!
//! - **ghost text** — the suggestion suffix is drawn dim after the cursor
//!   and never enters the buffer (zsh `POSTDISPLAY`,
//!   `widgets.zsh:103`);
//! - **widget action classes** — buffer modifications refetch the
//!   suggestion, cursor movement clears it, `modify/clear/accept/
//!   partial_accept` from `widgets.zsh:63-93`;
//! - **accept** — Right arrow at end of buffer or Tab accepts the whole
//!   suggestion; Ctrl-Right or Alt-f accepts one word (`partial_accept`,
//!   `widgets.zsh:121-142`);
//! - **history browsing** — Up/Down walk prefix-filtered history, restoring
//!   the typed prefix when browsed past the newest match.
//!
//! Structure: [`EditorCore`] is a pure state machine (unit-testable without
//! a terminal); `run_tty` wraps it with the crossterm event source, raw
//! mode, bracketed paste and the Drop-guarded teardown. Non-TTY sessions
//! never reach this module — the session falls back to the cooked
//! [`crate::input::LineReader`] pipe path.

mod core;
mod keymap;
mod r#loop;

pub use core::{partial_word_bytes, CoreAction, EditorCore, EditorSnapshot, PromptEvent};
pub use keymap::{map_key_event, Key};
pub use r#loop::{run_tty, terminal_cols};
