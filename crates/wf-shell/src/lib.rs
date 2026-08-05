//! Shell / terminal engine crate.
//!
//! Low-level shell execution primitives shared by tool wrappers (wf-tools),
//! runtime wiring (wf-runtime) and future terminal consumers: shell
//! detection, a stateless single-command runner, a background session engine
//! (pipe / PTY backends with incremental output), the shell event sink trait
//! and the command safety policy.

pub mod command_safety;
pub mod config;
pub mod engine;
pub mod error;
pub mod event_sink;
pub mod runner;
pub mod shell_detector;

pub use config::ShellToolConfig;
pub use engine::BackgroundShellStore;
pub use error::{ShellError, ShellResult};
pub use event_sink::ShellEventSink;
