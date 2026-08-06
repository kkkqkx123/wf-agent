//! Background shell engine (module facade).
//!
//! The engine implementation was split into focused modules — the session
//! store ([`crate::store`]), terminal session records
//! ([`crate::terminal_session`]), command sessions ([`crate::session`]),
//! subprocess backends ([`crate::backend`]), the output pipeline
//! ([`crate::output_buffer`], [`crate::utf8`], [`crate::line_dispatcher`]) and
//! the drain coordination ([`crate::drain`]). This module re-exports the
//! public engine API so existing `wf_shell::engine::*` imports keep working.

pub use crate::backend::SessionMode;
pub use crate::session::ShellSession;
pub use crate::store::{
    BackgroundShellStore, GetOrCreateResult, SessionCreateOptions, SpawnOptions,
};
pub use crate::terminal_session::{SessionStatus, TerminalSession};
