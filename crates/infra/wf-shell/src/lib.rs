//! Shell / terminal engine crate.
//!
//! Low-level shell execution primitives shared by tool wrappers (wf-tools),
//! runtime wiring (wf-runtime) and future terminal consumers: shell
//! detection, a stateless single-command runner, a background session engine
//! (pipe / PTY backends with incremental output), the shell event sink trait
//! and the command safety policy.
//!
//! ## Env inheritance
//!
//! Every spawned shell inherits the parent process environment and then
//! overlays the session/command env; a variable set in the overlay replaces
//! the inherited value, including
//! special variables such as `PATH`. All spawn paths (stateless runner and
//! session engine) go through [`crate::spawn`], so this rule is uniform.
//!
//! ## PTY support
//!
//! Interactive sessions run on a real terminal (PTY) via `portable-pty`,
//! which is always compiled in (not feature-gated). At runtime the backend is
//! governed by one knob:
//!
//! - [`crate::config::ShellToolConfig::pty_enabled`]: store-level switch for
//!   whether the PTY backend may be used at all (default `true`).
//!
//! A session only uses PTY mode when `pty_enabled` is true and the session
//! requests it via [`crate::engine::SessionCreateOptions::interactive`] (or
//! `force_pty`); every other combination falls back to pipe mode.

pub mod backend;
pub mod command_safety;
pub mod config;
pub mod drain;
pub mod engine;
pub mod error;
pub mod event_sink;
pub mod line_dispatcher;
pub mod output_buffer;
pub mod runner;
pub mod session;
pub mod shell_detector;
pub mod spawn;
pub mod store;
pub mod terminal_session;
pub mod utf8;

pub use config::ShellToolConfig;
pub use engine::BackgroundShellStore;
pub use error::{ShellError, ShellResult};
pub use event_sink::ShellEventSink;
