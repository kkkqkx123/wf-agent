use serde::{Deserialize, Serialize};

/// Transport and retry oriented error label persisted on error records.
/// Used for aggregation, routing projection and user facing suggestions.
/// When the same HTTP status arises from different sources the source
/// specific analysis keeps the transient type (RateLimited, ServiceUnavailable,
/// Timeout) instead of the source type so throttling and outages are never
/// counted as plain timeouts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    ToolError,
    LlmError,
    Timeout,
    /// HTTP 429 style quota exhaustion: transient, needs backoff not a
    /// plain timeout re-run.
    RateLimited,
    /// HTTP 5xx style upstream failure: transient, retried with backoff.
    ServiceUnavailable,
    Validation,
    Internal,
    Interruption,
}

impl ErrorType {
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ErrorType::Timeout | ErrorType::RateLimited | ErrorType::ServiceUnavailable
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecoveryAction {
    Retry,
    ManualIntervention,
    Abort,
}

impl RecoveryAction {
    pub fn is_retry(&self) -> bool {
        matches!(self, RecoveryAction::Retry)
    }

    pub fn is_terminal(&self) -> bool {
        !self.is_retry()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorCause {
    pub reason: String,
}

/// Domain origin of a failure used for retry policy and transport category.
/// Kept separate from ErrorType on purpose: kind answers where the failure
/// comes from, type answers how the engine should treat it for retry and
/// routing. Source specific analysis may pair one kind with different types
/// (for example network failures surface as ToolError or LlmError), so the
/// mapping below is only the stable fallback when the source is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Validation,
    Execution,
    NotFound,
    AgentCheckpoint,
    StateManagement,
    Tool,
    Network,
    Resource,
    Timeout,
    RateLimited,
    AuthError,
    ServiceUnavailable,
    General,
}

impl ErrorKind {
    pub fn is_non_retryable(&self) -> bool {
        matches!(
            self,
            ErrorKind::Validation
                | ErrorKind::NotFound
                | ErrorKind::StateManagement
                | ErrorKind::AuthError
        )
    }

    pub fn default_error_type(&self) -> ErrorType {
        match self {
            ErrorKind::Validation | ErrorKind::NotFound | ErrorKind::AuthError => {
                ErrorType::Validation
            }
            ErrorKind::Timeout => ErrorType::Timeout,
            ErrorKind::RateLimited => ErrorType::RateLimited,
            ErrorKind::ServiceUnavailable | ErrorKind::Network | ErrorKind::Resource => {
                ErrorType::ServiceUnavailable
            }
            ErrorKind::Tool => ErrorType::ToolError,
            ErrorKind::Execution
            | ErrorKind::AgentCheckpoint
            | ErrorKind::StateManagement
            | ErrorKind::General => ErrorType::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_types_are_retry_shaped() {
        assert!(ErrorType::Timeout.is_transient());
        assert!(ErrorType::RateLimited.is_transient());
        assert!(ErrorType::ServiceUnavailable.is_transient());
        assert!(!ErrorType::ToolError.is_transient());
        assert!(!ErrorType::LlmError.is_transient());
        assert!(!ErrorType::Validation.is_transient());
        assert!(!ErrorType::Internal.is_transient());
        assert!(!ErrorType::Interruption.is_transient());
    }

    #[test]
    fn recovery_action_retry_is_single_decision_bit() {
        assert!(RecoveryAction::Retry.is_retry());
        assert!(!RecoveryAction::Retry.is_terminal());
        assert!(!RecoveryAction::Abort.is_retry());
        assert!(RecoveryAction::Abort.is_terminal());
        assert!(!RecoveryAction::ManualIntervention.is_retry());
        assert!(RecoveryAction::ManualIntervention.is_terminal());
    }

    #[test]
    fn non_retryable_kinds_are_deterministic() {
        assert!(ErrorKind::Validation.is_non_retryable());
        assert!(ErrorKind::NotFound.is_non_retryable());
        assert!(ErrorKind::StateManagement.is_non_retryable());
        assert!(ErrorKind::AuthError.is_non_retryable());
        assert!(!ErrorKind::Timeout.is_non_retryable());
        assert!(!ErrorKind::Network.is_non_retryable());
        assert!(!ErrorKind::Tool.is_non_retryable());
    }

    #[test]
    fn default_error_type_covers_every_kind() {
        assert_eq!(
            ErrorKind::Validation.default_error_type(),
            ErrorType::Validation
        );
        assert_eq!(ErrorKind::Timeout.default_error_type(), ErrorType::Timeout);
        assert_eq!(
            ErrorKind::RateLimited.default_error_type(),
            ErrorType::RateLimited
        );
        assert_eq!(
            ErrorKind::Network.default_error_type(),
            ErrorType::ServiceUnavailable
        );
        assert_eq!(ErrorKind::Tool.default_error_type(), ErrorType::ToolError);
        assert_eq!(
            ErrorKind::Execution.default_error_type(),
            ErrorType::Internal
        );
        assert_eq!(ErrorKind::General.default_error_type(), ErrorType::Internal);
    }
}
