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
//! overlays the session/command env (`{...process.env, ...env}` in TS terms);
//! a variable set in the overlay replaces the inherited value, including
//! special variables such as `PATH`. All spawn paths (stateless runner and
//! session engine) go through [`crate::spawn`], so this rule is uniform.
//!
//! ## PTY support
//!
//! The `pty` crate feature is **disabled by default**. It enables a real
//! terminal (PTY) backend for interactive sessions via `portable-pty`; without
//! it, interactive requests silently fall back to the pipe backend.
//!
//! At runtime the backend is governed by two knobs:
//!
//! - [`crate::config::ShellToolConfig::pty_enabled`]: store-level switch for
//!   whether the PTY backend may be used at all (default `true`).
//! - [`crate::engine::SessionCreateOptions::interactive`] (or
//!   `backend_shell`'s `interactive`/`force_pty`): per-session request for a
//!   real terminal.
//!
//! A session only uses PTY mode when the `pty` feature is compiled in,
//! `pty_enabled` is true and the session requests it; every other combination
//! falls back to pipe mode. Feature users should enable `pty` on the consuming
//! crate (e.g. `wf-tools` with `features = ["pty"]`).

pub mod command_safety;
pub mod config;
pub mod engine;
pub mod error;
pub mod event_sink;
pub mod runner;
pub mod shell_detector;
pub mod spawn;

pub use config::ShellToolConfig;
pub use engine::BackgroundShellStore;
pub use error::{ShellError, ShellResult};
pub use event_sink::ShellEventSink;
