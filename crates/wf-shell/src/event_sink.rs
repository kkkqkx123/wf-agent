//! Event sink abstraction for shell session events.
//!
//! A [`ShellEventSink`] receives session lifecycle and per-line output events
//! (aligned with the TS terminal service `output:received` events). The trait
//! lives in `wf-tools` so the crate does not depend on `wf-core`; upper
//! crates (e.g. `wf-runtime`) provide an implementation that bridges into the
//! `wf_core::EventBus`.

/// Receives shell session lifecycle and output events. All methods default to
/// no-ops; the session store only dispatches events when a sink is registered
/// (via [`crate::config::ShellToolConfig`]) and output events are enabled.
pub trait ShellEventSink: Send + Sync {
    /// A session was created, or an existing idle session was reused.
    fn on_session_created(&self, _session_id: &str, _reused: bool, _task_id: Option<&str>) {}

    /// A command started running in a session.
    fn on_command_started(&self, _session_id: &str, _task_id: Option<&str>, _command: &str) {}

    /// A complete output line was received from the session. Empty lines are
    /// skipped; CRLF is normalized on the PTY path, raw bytes are preserved
    /// on the pipe path (same as the output buffer).
    fn on_output(&self, _session_id: &str, _task_id: Option<&str>, _line: &str) {}

    /// A command finished (exited or was terminated).
    fn on_command_completed(
        &self,
        _session_id: &str,
        _task_id: Option<&str>,
        _command: &str,
        _exit_code: Option<i32>,
        _success: bool,
    ) {
    }

    /// A session was terminated (killed and removed from the store).
    fn on_session_terminated(&self, _session_id: &str, _task_id: Option<&str>) {}
}
