use crate::error::AgentError;

/// How a run error settles the terminal state of an agent loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettleKind {
    Timeout,
    Cancel,
    Fail,
}

/// Decide the terminal settle for a run error. A cancelled run (explicit
/// cancellation error, or an in-flight provider/tool cancellation) settles as
/// `Cancelled`, never `Failed`. When the host runtime is closing
/// (`active_shutdown`), an in-flight run must not be turned into a spurious
/// `Failed`: cancel it so no failure is dispatched or persisted for an
/// execution the user deliberately left.
pub(super) fn settle_kind(err: &AgentError, active_shutdown: bool) -> SettleKind {
    if active_shutdown {
        SettleKind::Cancel
    } else if matches!(err, AgentError::ExecutionTimeout(_)) {
        SettleKind::Timeout
    } else if matches!(
        err,
        AgentError::Cancelled(_) | AgentError::LlmError(wf_llm::error::LlmError::Cancelled)
    ) {
        SettleKind::Cancel
    } else {
        SettleKind::Fail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settle_kind_fails_on_generic_errors_when_running() {
        let err = AgentError::Internal("boom".to_string());
        assert_eq!(settle_kind(&err, false), SettleKind::Fail);
    }

    #[test]
    fn settle_kind_timeouts_only_while_running() {
        let err = AgentError::ExecutionTimeout("slow".to_string());
        assert_eq!(settle_kind(&err, false), SettleKind::Timeout);
        // While the host is closing, even a timeout settles as a cancel so
        // no spurious failure is recorded for a run the user left behind.
        assert_eq!(settle_kind(&err, true), SettleKind::Cancel);
    }

    #[test]
    fn settle_kind_cancels_every_error_during_active_shutdown() {
        let err = AgentError::Internal("teardown".to_string());
        assert_eq!(settle_kind(&err, true), SettleKind::Cancel);
    }

    /// A circuit-breaker exit is a failure of the run, not a timeout or a
    /// cancellation: it stops on its own terms, so the run must settle as
    /// failed.
    #[test]
    fn settle_kind_fails_on_a_tripped_error_pattern() {
        let err = AgentError::ErrorPatternTripped("Timeout recurred 3 times".to_string());
        assert_eq!(settle_kind(&err, false), SettleKind::Fail);
    }
}
