//! wf-cli-demo: runnable demos for every wf-agent CLI implementation.
//!
//! This crate is demo-only. It contains no product logic: each `examples/`
//! target showcases one CLI form so developers can inspect and run each
//! implementation without touching product crates.
//!
//! Layout keeps the three forms separate:
//! - `examples/tui/` covers the full TUI (`wf-tui`, ratatui alternate screen).
//! - `examples/mini/` covers the lightweight line-discipline session
//!   (`wf-mini`: stdout assistant text, stderr diagnostics, `> ` prompt).
//! - `examples/headless/` covers the non-interactive run (`wf run` through
//!   `wf-cli-shared`: format layer times sink layer, diagnostics on stderr).
//!
//! Shared preview plumbing lives in this library so each example stays
//! focused on one scenario: `mini_render` (markdown and buffering),
//! `mini_turn` (turn routing), `mini_input` (prompt layer), `mini_memory`
//! (session memory), `headless_common` (format and sink plumbing), and
//! `emit` (golden captures under `outputs/<form>/`).

pub mod emit;
pub mod headless_common;
pub mod mini_input;
pub mod mini_memory;
pub mod mini_render;
pub mod mini_turn;
