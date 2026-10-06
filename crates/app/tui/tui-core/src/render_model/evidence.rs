//! Failure evidence bundle for frame assertions.

use super::event_recording::RecordedEvent;

/// Failure evidence bundle: everything needed to reproduce a failed frame
/// assertion without rerunning the live session. Built only on failure, so
/// the success path pays nothing.
#[derive(Debug, Default)]
pub struct TestEvidence {
    /// Bundle name (usually the failing test name).
    pub name: String,
    /// Interaction sequence leading to the failure.
    pub events: Vec<RecordedEvent>,
    /// Frame snapshots (one entry per frame, headless rows joined).
    pub frames: Vec<String>,
    /// Assertion messages that failed.
    pub assertions: Vec<String>,
    /// Captured log lines.
    pub logs: Vec<String>,
    /// Captured standard output lines.
    pub stdout: Vec<String>,
    /// Captured standard error lines.
    pub stderr: Vec<String>,
    /// Sealed bundles reject further mutation; `seal` freezes the bundle
    /// for handoff to CI artifacts.
    sealed: bool,
}

impl TestEvidence {
    /// New empty bundle.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Record an event. Ignored once sealed.
    pub fn add_event(&mut self, event: RecordedEvent) {
        if !self.sealed {
            self.events.push(event);
        }
    }

    /// Record a frame snapshot. Ignored once sealed.
    pub fn add_frame(&mut self, frame: impl Into<String>) {
        if !self.sealed {
            self.frames.push(frame.into());
        }
    }

    /// Record a failed assertion. Ignored once sealed.
    pub fn add_assertion(&mut self, assertion: impl Into<String>) {
        if !self.sealed {
            self.assertions.push(assertion.into());
        }
    }

    /// Record a log line. Ignored once sealed.
    pub fn add_log(&mut self, log: impl Into<String>) {
        if !self.sealed {
            self.logs.push(log.into());
        }
    }

    /// Record a standard output line. Ignored once sealed.
    pub fn add_stdout(&mut self, line: impl Into<String>) {
        if !self.sealed {
            self.stdout.push(line.into());
        }
    }

    /// Record a standard error line. Ignored once sealed.
    pub fn add_stderr(&mut self, line: impl Into<String>) {
        if !self.sealed {
            self.stderr.push(line.into());
        }
    }

    /// True when at least one assertion failed.
    pub fn failed(&self) -> bool {
        !self.assertions.is_empty()
    }

    /// One-line summary for test output.
    pub fn summary(&self) -> String {
        format!(
            "{}: events={} frames={} assertions={} logs={} stdout={} stderr={}",
            self.name,
            self.events.len(),
            self.frames.len(),
            self.assertions.len(),
            self.logs.len(),
            self.stdout.len(),
            self.stderr.len(),
        )
    }

    /// Seal the bundle: further `add_*` calls are ignored. Returns the
    /// summary for CI logs.
    pub fn seal(&mut self) -> String {
        self.sealed = true;
        self.summary()
    }

    /// Whether the bundle is sealed.
    pub fn is_sealed(&self) -> bool {
        self.sealed
    }
}
