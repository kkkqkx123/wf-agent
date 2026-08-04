use std::sync::Arc;
use wf_types::checkpoint::{
    CheckpointErrorContext, CheckpointErrorHandlingConfig, CheckpointErrorHandlingResult,
    CheckpointErrorStrategy, UnifiedCheckpointPolicy,
};

use crate::error::CheckpointError;

/// Outcome of an error handling decision, aligned with the TS
/// `CheckpointErrorHandlingResult` (`{shouldRethrow, handled}`).
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

pub type CheckpointErrorCallback = dyn Fn(&CheckpointErrorContext, &CheckpointError) + Send + Sync;

/// Error handler for checkpoint operations, aligned with the TS
/// `CheckpointErrorHandler`:
///
/// - `silent`   — debug-log and swallow the error;
/// - `warn`     — warn-log and swallow the error (default);
/// - `strict`   — error-log and rethrow;
/// - `callback` — delegate to the user-provided callback (callback failure
///   rethrows).
///
/// The `failOnCheckpointError` / `retryOnFailure` / `maxRetries` fields of
/// the unified policy drive the retry and strictness behavior.
pub struct CheckpointErrorHandler {
    strategy: CheckpointErrorStrategy,
    fail_on_checkpoint_error: bool,
    retry_on_failure: bool,
    max_retries: u32,
    callback: Option<Arc<CheckpointErrorCallback>>,
}

impl CheckpointErrorHandler {
    pub fn new(strategy: CheckpointErrorStrategy) -> Self {
        Self {
            strategy,
            fail_on_checkpoint_error: false,
            retry_on_failure: true,
            max_retries: 3,
            callback: None,
        }
    }

    /// Build from a unified policy's error handling config, falling back to
    /// the TS defaults (`failOnCheckpointError: false`,
    /// `retryOnFailure: true`, `maxRetries: 3`).
    pub fn from_policy(policy: &UnifiedCheckpointPolicy) -> Self {
        Self::from_config(policy.error_handling.as_ref())
    }

    pub fn from_config(config: Option<&CheckpointErrorHandlingConfig>) -> Self {
        let mut handler = Self::new(CheckpointErrorStrategy::Warn);
        if let Some(config) = config {
            handler.fail_on_checkpoint_error = config.fail_on_checkpoint_error.unwrap_or(false);
            handler.retry_on_failure = config.retry_on_failure.unwrap_or(true);
            handler.max_retries = config.max_retries.unwrap_or(3);
        }
        handler
    }

    pub fn with_callback(mut self, callback: Arc<CheckpointErrorCallback>) -> Self {
        self.callback = Some(callback);
        self
    }

    pub fn get_strategy(&self) -> &CheckpointErrorStrategy {
        &self.strategy
    }

    pub fn set_strategy(&mut self, strategy: CheckpointErrorStrategy) {
        self.strategy = strategy;
    }

    pub fn fail_on_checkpoint_error(&self) -> bool {
        self.fail_on_checkpoint_error
    }

    pub fn retry_on_failure(&self) -> bool {
        self.retry_on_failure
    }

    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// Handle a checkpoint error according to the configured strategy.
    /// Retry progress is tracked through the context attempt count.
    pub fn handle(
        &self,
        context: &CheckpointErrorContext,
        error: &CheckpointError,
    ) -> CheckpointErrorHandlingResult {
        let retry_count = context.attempt.min(self.max_retries);

        if self.fail_on_checkpoint_error {
            return CheckpointErrorHandlingResult {
                recovered: false,
                retry_count,
                error: Some(error.to_string()),
            };
        }

        let log_context = format!(
            "operation={} checkpoint_id={:?}",
            context.operation, context.checkpoint_id
        );

        match self.strategy {
            CheckpointErrorStrategy::Silent => {
                tracing::debug!(target: "wf_checkpoint", "{log_context}: {error}");
                CheckpointErrorHandlingResult {
                    recovered: true,
                    retry_count,
                    error: None,
                }
            }
            CheckpointErrorStrategy::Warn => {
                // Warn never rethrows: checkpoint failures are non-fatal for
                // the execution unless failOnCheckpointError is set.
                tracing::warn!(target: "wf_checkpoint", "{log_context}: {error}");
                CheckpointErrorHandlingResult {
                    recovered: true,
                    retry_count,
                    error: None,
                }
            }
            CheckpointErrorStrategy::Strict => {
                tracing::error!(target: "wf_checkpoint", "{log_context}: {error}");
                CheckpointErrorHandlingResult {
                    recovered: false,
                    retry_count,
                    error: Some(error.to_string()),
                }
            }
            CheckpointErrorStrategy::Callback => match &self.callback {
                Some(callback) => {
                    callback(context, error);
                    CheckpointErrorHandlingResult {
                        recovered: true,
                        retry_count,
                        error: None,
                    }
                }
                None => {
                    tracing::warn!(target: "wf_checkpoint", "{log_context}: no callback registered, swallowing: {error}");
                    CheckpointErrorHandlingResult {
                        recovered: true,
                        retry_count,
                        error: None,
                    }
                }
            },
        }
    }

    /// Convenience wrapper mapping the result to a rethrow decision.
    pub fn decide(
        &self,
        context: &CheckpointErrorContext,
        error: &CheckpointError,
    ) -> ErrorHandlingOutcome {
        let result = self.handle(context, error);
        if result.recovered && !self.fail_on_checkpoint_error {
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
        attempt: u32,
    ) -> CheckpointErrorContext {
        CheckpointErrorContext {
            operation: operation.into(),
            checkpoint_id,
            message,
            attempt,
        }
    }
}

impl Default for CheckpointErrorHandler {
    fn default() -> Self {
        Self::new(CheckpointErrorStrategy::Warn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error() -> CheckpointError {
        CheckpointError::Coordinator("boom".to_string())
    }

    fn context(attempt: u32) -> CheckpointErrorContext {
        CheckpointErrorContext {
            operation: "create".to_string(),
            checkpoint_id: Some("cp-1".to_string()),
            message: None,
            attempt,
        }
    }

    #[test]
    fn warn_default_swallows() {
        let handler = CheckpointErrorHandler::default();
        let result = handler.handle(&context(0), &error());
        assert!(result.recovered);
        assert!(result.error.is_none());
        assert!(!handler.decide(&context(0), &error()).should_rethrow);
    }

    #[test]
    fn strict_rethrows() {
        let handler = CheckpointErrorHandler::new(CheckpointErrorStrategy::Strict);
        assert!(handler.decide(&context(0), &error()).should_rethrow);
        assert!(!handler.handle(&context(0), &error()).recovered);
    }

    #[test]
    fn silent_swallows_quietly() {
        let handler = CheckpointErrorHandler::new(CheckpointErrorStrategy::Silent);
        assert!(!handler.decide(&context(0), &error()).should_rethrow);
    }

    #[test]
    fn fail_on_checkpoint_error_rethrows() {
        let mut handler = CheckpointErrorHandler::default();
        handler.fail_on_checkpoint_error = true;
        assert!(handler.decide(&context(0), &error()).should_rethrow);
    }

    #[test]
    fn retry_tracks_attempts() {
        let handler = CheckpointErrorHandler::new(CheckpointErrorStrategy::Warn);
        let result = handler.handle(&context(1), &error());
        assert!(result.recovered, "warn always swallows");
        assert_eq!(result.retry_count, 1);
        assert_eq!(handler.max_retries(), 3);
        assert!(handler.retry_on_failure());

        let exhausted = handler.handle(&context(4), &error());
        assert_eq!(
            exhausted.retry_count, 3,
            "retry count capped at max_retries"
        );
    }

    #[test]
    fn callback_strategy_invokes_user_callback() {
        let called = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let callback: Arc<CheckpointErrorCallback> = Arc::new({
            let called = called.clone();
            move |_ctx, _err| {
                called.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        let handler =
            CheckpointErrorHandler::new(CheckpointErrorStrategy::Callback).with_callback(callback);
        let result = handler.handle(&context(0), &error());
        assert!(result.recovered);
        assert_eq!(called.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn callback_without_registration_warns_and_swallows() {
        let handler = CheckpointErrorHandler::new(CheckpointErrorStrategy::Callback);
        let result = handler.handle(&context(0), &error());
        assert!(result.recovered);
    }

    #[test]
    fn from_policy_reads_error_handling_config() {
        let policy = UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![],
            content: None,
            retention: None,
            error_handling: Some(CheckpointErrorHandlingConfig {
                fail_on_checkpoint_error: Some(true),
                retry_on_failure: Some(false),
                max_retries: Some(5),
            }),
        };
        let handler = CheckpointErrorHandler::from_policy(&policy);
        assert!(handler.fail_on_checkpoint_error());
        assert!(handler.decide(&context(0), &error()).should_rethrow);
    }

    #[test]
    fn strategy_can_be_changed() {
        let mut handler = CheckpointErrorHandler::default();
        handler.set_strategy(CheckpointErrorStrategy::Strict);
        assert_eq!(handler.get_strategy(), &CheckpointErrorStrategy::Strict);
    }
}
