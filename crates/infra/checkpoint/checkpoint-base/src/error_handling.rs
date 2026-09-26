use wf_types::checkpoint::{
    CheckpointErrorContext, CheckpointErrorHandlingConfig, CheckpointErrorHandlingResult,
    UnifiedCheckpointPolicy,
};

use crate::error::CheckpointError;

/// Outcome of an error handling decision (`{shouldRethrow, handled}`).
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorHandlingOutcome {
    pub should_rethrow: bool,
    pub handled: bool,
}

impl ErrorHandlingOutcome {
    pub fn rethrow() -> Self {
        Self {
            should_rethrow: true,
            handled: false,
        }
    }

    pub fn swallowed() -> Self {
        Self {
            should_rethrow: false,
            handled: true,
        }
    }
}

/// Error handler for checkpoint operations.
///
/// A checkpoint failure is either made visible (`fail_on_checkpoint_error`
/// is set: error-log and rethrow to the caller) or warn-logged and swallowed
/// so the execution continues without that checkpoint. Automatic retry is
/// deliberately not offered: a failed write is recorded, never replayed.
pub struct CheckpointErrorHandler {
    fail_on_checkpoint_error: bool,
}

impl CheckpointErrorHandler {
    pub fn new(fail_on_checkpoint_error: bool) -> Self {
        Self {
            fail_on_checkpoint_error,
        }
    }

    /// Build from a unified policy's error handling config.
    ///
    /// An absent config (or absent field) means checkpoint write failures
    /// are warn-logged and swallowed, matching `default()`.
    pub fn from_policy(policy: &UnifiedCheckpointPolicy) -> Self {
        Self::from_config(policy.error_handling.as_ref())
    }

    pub fn from_config(config: Option<&CheckpointErrorHandlingConfig>) -> Self {
        Self::new(
            config
                .and_then(|c| c.fail_on_checkpoint_error)
                .unwrap_or(false),
        )
    }

    pub fn fail_on_checkpoint_error(&self) -> bool {
        self.fail_on_checkpoint_error
    }

    /// Handle a checkpoint error: log it and report whether the execution
    /// can continue.
    pub fn handle(
        &self,
        context: &CheckpointErrorContext,
        error: &CheckpointError,
    ) -> CheckpointErrorHandlingResult {
        let log_context = format!(
            "operation={} checkpoint_id={:?}",
            context.operation, context.checkpoint_id
        );

        if self.fail_on_checkpoint_error {
            tracing::error!(target: "wf_checkpoint", "{log_context}: {error}");
            return CheckpointErrorHandlingResult {
                recovered: false,
                error: Some(error.to_string()),
            };
        }

        // Swallowed failures are non-fatal for the execution: the run
        // continues without this checkpoint.
        tracing::warn!(target: "wf_checkpoint", "{log_context}: {error}");
        CheckpointErrorHandlingResult {
            recovered: true,
            error: None,
        }
    }

    /// Convenience wrapper mapping the result to a rethrow decision.
    pub fn decide(
        &self,
        context: &CheckpointErrorContext,
        error: &CheckpointError,
    ) -> ErrorHandlingOutcome {
        let result = self.handle(context, error);
        if result.recovered {
            ErrorHandlingOutcome::swallowed()
        } else {
            ErrorHandlingOutcome::rethrow()
        }
    }

    /// Build the correlation context for an operation.
    pub fn context(
        &self,
        operation: impl Into<String>,
        checkpoint_id: Option<String>,
        message: Option<String>,
    ) -> CheckpointErrorContext {
        CheckpointErrorContext {
            operation: operation.into(),
            checkpoint_id,
            message,
        }
    }
}

impl Default for CheckpointErrorHandler {
    fn default() -> Self {
        Self::new(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error() -> CheckpointError {
        CheckpointError::Coordinator("boom".to_string())
    }

    fn context() -> CheckpointErrorContext {
        CheckpointErrorContext {
            operation: "create".to_string(),
            checkpoint_id: Some("cp-1".to_string()),
            message: None,
        }
    }

    #[test]
    fn default_swallows_with_warning() {
        let handler = CheckpointErrorHandler::default();
        let result = handler.handle(&context(), &error());
        assert!(result.recovered);
        assert!(result.error.is_none());
        assert!(!handler.decide(&context(), &error()).should_rethrow);
    }

    #[test]
    fn fail_on_checkpoint_error_rethrows() {
        let handler = CheckpointErrorHandler::new(true);
        let result = handler.handle(&context(), &error());
        assert!(!result.recovered);
        assert!(result.error.is_some());
        assert!(handler.decide(&context(), &error()).should_rethrow);
    }

    #[test]
    fn absent_config_matches_default() {
        // "not configured" must mean one behavior only: swallow.
        let from_none = CheckpointErrorHandler::from_config(None);
        assert!(!from_none.fail_on_checkpoint_error());
        assert!(!from_none.decide(&context(), &error()).should_rethrow);

        let policy = UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![],
            content: None,
            retention: None,
            error_handling: None,
        };
        let from_policy = CheckpointErrorHandler::from_policy(&policy);
        assert!(!from_policy.fail_on_checkpoint_error());
    }

    #[test]
    fn from_policy_reads_single_field() {
        let policy = UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![],
            content: None,
            retention: None,
            error_handling: Some(CheckpointErrorHandlingConfig {
                fail_on_checkpoint_error: Some(true),
            }),
        };
        let handler = CheckpointErrorHandler::from_policy(&policy);
        assert!(handler.fail_on_checkpoint_error());
        assert!(handler.decide(&context(), &error()).should_rethrow);
    }

    #[test]
    fn explicit_false_keeps_swallow_behavior() {
        let policy = UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![],
            content: None,
            retention: None,
            error_handling: Some(CheckpointErrorHandlingConfig {
                fail_on_checkpoint_error: Some(false),
            }),
        };
        let handler = CheckpointErrorHandler::from_policy(&policy);
        assert!(!handler.fail_on_checkpoint_error());
        assert!(!handler.decide(&context(), &error()).should_rethrow);
    }
}
