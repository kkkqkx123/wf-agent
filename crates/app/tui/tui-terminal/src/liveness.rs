//! Terminal liveness: orphan detection for detached UI clients.
//!
//! A UI client holding no session work must exit when its input reaches EOF
//! and the control terminal is gone; otherwise it idles forever holding
//! memory. The probe is pure state so tests drive it without a TTY.

/// Liveness observation for one poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LivenessSample {
    pub input_eof: bool,
    pub control_tty_gone: bool,
    pub has_live_session: bool,
}

/// True when the client is orphaned and must exit.
pub fn is_orphaned(sample: LivenessSample) -> bool {
    sample.input_eof && sample.control_tty_gone && !sample.has_live_session
}

/// Edge-triggered orphan probe: reports true once when the sample first
/// becomes orphaned.
#[derive(Debug, Default)]
pub struct LivenessProbe {
    reported: bool,
}

impl LivenessProbe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn poll(&mut self, sample: LivenessSample) -> bool {
        if self.reported {
            return false;
        }
        if is_orphaned(sample) {
            self.reported = true;
            return true;
        }
        false
    }

    pub fn reset(&mut self) {
        self.reported = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_client_is_not_orphaned() {
        assert!(!is_orphaned(LivenessSample {
            input_eof: false,
            control_tty_gone: false,
            has_live_session: true,
        }));
    }

    #[test]
    fn eof_without_tty_and_work_is_orphaned() {
        assert!(is_orphaned(LivenessSample {
            input_eof: true,
            control_tty_gone: true,
            has_live_session: false,
        }));
    }

    #[test]
    fn live_session_blocks_orphan_exit() {
        assert!(!is_orphaned(LivenessSample {
            input_eof: true,
            control_tty_gone: true,
            has_live_session: true,
        }));
    }

    #[test]
    fn probe_reports_orphan_once() {
        let mut probe = LivenessProbe::new();
        let sample = LivenessSample {
            input_eof: true,
            control_tty_gone: true,
            has_live_session: false,
        };
        assert!(probe.poll(sample));
        assert!(!probe.poll(sample));
        probe.reset();
        assert!(probe.poll(sample));
    }
}
