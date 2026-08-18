use std::collections::HashMap;

use wf_common::error_chain::{ErrorMetadata, ErrorRecord};
use wf_execution_shared::error::ExecutionSharedError;
use wf_llm::error::LlmError;
use wf_tools::error::ToolError;
use wf_types::errors::{ErrorCause, ErrorKind, ErrorType, RecoveryAction};

use crate::error::AgentError;

/// Structured analysis of an agent error: error kind, retryability,
/// recommended action and the contextual message.
#[derive(Debug, Clone)]
pub struct ErrorAnalysis {
    pub kind: ErrorKind,
    pub error_type: ErrorType,
    pub retryable: bool,
    pub recovery_action: RecoveryAction,
    pub message: String,
    pub cause: Option<ErrorCause>,
}

impl ErrorAnalysis {
    /// Build an ErrorRecord for persistence into entity state / snapshots.
    /// When no parent record is provided, this is treated as the root error.
    pub fn to_error_record(&self, execution_id: &str, node_id: Option<String>) -> ErrorRecord {
        let id = wf_common::generate_id();
        ErrorRecord {
            id: id.clone(),
            execution_id: execution_id.to_string(),
            error: self.message.clone(),
            error_type: Some(self.error_type.clone()),
            timestamp: wf_common::now(),
            node_id,
            parent_error_id: None,
            error_chain: vec![id.clone()],
            root_cause_id: id,
            caused_by: self.cause.clone(),
            is_recoverable: self.retryable,
            recovery_action: Some(self.recovery_action.clone()),
        }
    }

    /// Build an ErrorRecord linked to a parent error, forming an error chain.
    /// The parent's `error_chain` is extended, `root_cause_id` is preserved,
    /// and `parent_error_id` points to the parent.
    pub fn to_chained_error_record(
        &self,
        execution_id: &str,
        node_id: Option<String>,
        parent: &ErrorRecord,
    ) -> ErrorRecord {
        let id = wf_common::generate_id();
        let mut error_chain = parent.error_chain.clone();
        error_chain.push(id.clone());
        ErrorRecord {
            id,
            execution_id: execution_id.to_string(),
            error: self.message.clone(),
            error_type: Some(self.error_type.clone()),
            timestamp: wf_common::now(),
            node_id,
            parent_error_id: Some(parent.id.clone()),
            error_chain,
            root_cause_id: parent.root_cause_id.clone(),
            caused_by: self.cause.clone(),
            is_recoverable: self.retryable,
            recovery_action: Some(self.recovery_action.clone()),
        }
    }

    pub fn to_error_metadata(&self) -> ErrorMetadata {
        ErrorMetadata {
            error_type: Some(self.error_type.clone()),
            caused_by: self.cause.clone(),
            is_recoverable: self.retryable,
            recovery_action: Some(self.recovery_action.clone()),
        }
    }
}

pub fn http_status_to_kind(status: u16) -> ErrorKind {
    match status {
        400 => ErrorKind::Validation,
        401 | 403 => ErrorKind::AuthError,
        404 => ErrorKind::NotFound,
        429 => ErrorKind::RateLimited,
        500..=599 => ErrorKind::ServiceUnavailable,
        _ => ErrorKind::Network,
    }
}

pub fn tool_error_analysis(e: &ToolError) -> ErrorAnalysis {
    let (kind, error_type, retryable, recovery_action) = match e {
        ToolError::NotFound(_) => (
            ErrorKind::NotFound,
            ErrorType::ToolError,
            false,
            RecoveryAction::Abort,
        ),
        ToolError::ValidationFailed(_) => (
            ErrorKind::Validation,
            ErrorType::Validation,
            false,
            RecoveryAction::Abort,
        ),
        ToolError::Timeout { .. } => (
            ErrorKind::Timeout,
            ErrorType::Timeout,
            true,
            RecoveryAction::Retry,
        ),
        ToolError::RestError { status, .. } => {
            let kind = http_status_to_kind(*status);
            let retryable = matches!(*status, 429 | 500..=599);
            (
                kind,
                if retryable {
                    ErrorType::Timeout
                } else {
                    ErrorType::ToolError
                },
                retryable,
                if retryable {
                    RecoveryAction::Retry
                } else {
                    RecoveryAction::ManualIntervention
                },
            )
        }
        ToolError::HttpError(e) => match e.status() {
            Some(s) => {
                let kind = http_status_to_kind(s.as_u16());
                let retryable = matches!(s.as_u16(), 429 | 500..=599);
                (
                    kind,
                    if retryable {
                        ErrorType::Timeout
                    } else {
                        ErrorType::ToolError
                    },
                    retryable,
                    if retryable {
                        RecoveryAction::Retry
                    } else {
                        RecoveryAction::ManualIntervention
                    },
                )
            }
            None => (
                ErrorKind::Network,
                ErrorType::ToolError,
                true,
                RecoveryAction::Retry,
            ),
        },
        ToolError::ConnectionFailed { .. } | ToolError::TransportError(_) => (
            ErrorKind::Network,
            ErrorType::ToolError,
            true,
            RecoveryAction::Retry,
        ),
        _ => (
            ErrorKind::Tool,
            ErrorType::ToolError,
            true,
            RecoveryAction::Retry,
        ),
    };
    ErrorAnalysis {
        kind,
        error_type,
        retryable,
        recovery_action,
        message: e.to_string(),
        cause: None,
    }
}

pub fn llm_error_analysis(e: &LlmError) -> ErrorAnalysis {
    let (kind, error_type, retryable, recovery_action) = match e {
        LlmError::Timeout(_) => (
            ErrorKind::Timeout,
            ErrorType::Timeout,
            true,
            RecoveryAction::Retry,
        ),
        LlmError::HttpError(h) => match h.status() {
            Some(s) => {
                let kind = http_status_to_kind(s.as_u16());
                let retryable = matches!(s.as_u16(), 429 | 500..=599);
                (
                    kind,
                    if retryable {
                        ErrorType::Timeout
                    } else {
                        ErrorType::LlmError
                    },
                    retryable,
                    if retryable {
                        RecoveryAction::Retry
                    } else {
                        RecoveryAction::ManualIntervention
                    },
                )
            }
            None => (
                ErrorKind::Network,
                ErrorType::LlmError,
                true,
                RecoveryAction::Retry,
            ),
        },
        LlmError::AuthError(_) => (
            ErrorKind::AuthError,
            ErrorType::LlmError,
            false,
            RecoveryAction::ManualIntervention,
        ),
        LlmError::ProfileNotFound(_) => (
            ErrorKind::NotFound,
            ErrorType::Validation,
            false,
            RecoveryAction::Abort,
        ),
        LlmError::Cancelled => (
            ErrorKind::Execution,
            ErrorType::Interruption,
            false,
            RecoveryAction::Abort,
        ),
        _ => (
            ErrorKind::Network,
            ErrorType::LlmError,
            true,
            RecoveryAction::Retry,
        ),
    };
    ErrorAnalysis {
        kind,
        error_type,
        retryable,
        recovery_action,
        message: e.to_string(),
        cause: None,
    }
}

pub fn shared_error_analysis(e: &ExecutionSharedError) -> ErrorAnalysis {
    match e {
        ExecutionSharedError::StateError(_) => ErrorAnalysis {
            kind: ErrorKind::StateManagement,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
        ExecutionSharedError::ToolError(te) => tool_error_analysis(te),
        _ => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
    }
}

/// Classify an agent error into a structured analysis covering all AgentError
/// branches.
pub fn analyze_error(e: &AgentError) -> ErrorAnalysis {
    match e {
        AgentError::EntityError(_) => ErrorAnalysis {
            kind: ErrorKind::StateManagement,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
        AgentError::StateError(_) | AgentError::IllegalStateTransition(_) => ErrorAnalysis {
            kind: ErrorKind::StateManagement,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
        AgentError::CoordinatorError(_) | AgentError::Internal(_) => ErrorAnalysis {
            kind: ErrorKind::General,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
        AgentError::ExecutionError(_) | AgentError::ExecutionTimeout(_) => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Internal,
            retryable: true,
            recovery_action: RecoveryAction::Retry,
            message: e.to_string(),
            cause: None,
        },
        AgentError::ExecutionLimitReached(_) => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::ManualIntervention,
            message: e.to_string(),
            cause: None,
        },
        AgentError::HookError(_) => ErrorAnalysis {
            kind: ErrorKind::EventSystem,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::ManualIntervention,
            message: e.to_string(),
            cause: None,
        },
        AgentError::ToolError(te) => tool_error_analysis(te),
        AgentError::LlmError(le) => llm_error_analysis(le),
        AgentError::CheckpointError(_) => ErrorAnalysis {
            kind: ErrorKind::AgentCheckpoint,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::ManualIntervention,
            message: e.to_string(),
            cause: None,
        },
        AgentError::SharedError(se) => shared_error_analysis(se),
    }
}

// ── Error chain analysis utilities ──────────────────────────────────────────

/// Find the root cause error record from a list of error records.
/// Returns the record with the earliest timestamp (first error).
pub fn find_root_cause(records: &[ErrorRecord]) -> Option<&ErrorRecord> {
    records.iter().min_by_key(|r| r.timestamp)
}

/// Get the full error chain leading to the last error in the list.
/// Chains are built from `parent_error_id` links.
pub fn get_error_chain(records: &[ErrorRecord]) -> Vec<&ErrorRecord> {
    let last = match records.last() {
        Some(last) => last,
        None => return Vec::new(),
    };
    let mut chain: Vec<&ErrorRecord> = Vec::new();
    let mut current_id: Option<&String> = Some(&last.id);
    while let Some(id) = current_id {
        if let Some(record) = records.iter().find(|r| &r.id == id) {
            chain.push(record);
            current_id = record.parent_error_id.as_ref();
        } else {
            break;
        }
    }
    chain.reverse();
    chain
}

/// Analyze error patterns from a list of error records.
pub fn analyze_error_pattern(records: &[ErrorRecord]) -> ErrorPattern {
    let mut type_dist: HashMap<String, usize> = HashMap::new();
    let mut affected_nodes: Vec<String> = Vec::new();
    let mut recovery_count: HashMap<String, usize> = HashMap::new();
    let mut has_recoverable = false;

    for record in records {
        if let Some(ref error_type) = record.error_type {
            *type_dist.entry(format!("{:?}", error_type)).or_insert(0) += 1;
        }
        if let Some(ref node_id) = record.node_id {
            if !affected_nodes.contains(node_id) {
                affected_nodes.push(node_id.clone());
            }
        }
        if record.is_recoverable {
            has_recoverable = true;
        }
        if let Some(ref action) = record.recovery_action {
            *recovery_count.entry(format!("{:?}", action)).or_insert(0) += 1;
        }
    }

    let most_common_type = type_dist
        .iter()
        .max_by_key(|(_, count)| *count)
        .and_then(|(name, _)| match name.as_str() {
            "ToolError" => Some(ErrorType::ToolError),
            "LlmError" => Some(ErrorType::LlmError),
            "Timeout" => Some(ErrorType::Timeout),
            "Validation" => Some(ErrorType::Validation),
            "Internal" => Some(ErrorType::Internal),
            "Interruption" => Some(ErrorType::Interruption),
            _ => None,
        });

    ErrorPattern {
        total_errors: records.len(),
        type_distribution: type_dist,
        affected_nodes,
        most_common_type,
        has_recoverable,
        recovery_action_count: recovery_count,
    }
}

/// Get the recommended recovery action based on error pattern analysis.
pub fn get_recommended_recovery_action(records: &[ErrorRecord]) -> RecoveryAction {
    let pattern = analyze_error_pattern(records);
    if !pattern.has_recoverable {
        return RecoveryAction::Abort;
    }
    pattern
        .recovery_action_count
        .iter()
        .max_by_key(|(_, count)| *count)
        .and_then(|(name, _)| match name.as_str() {
            "Retry" => Some(RecoveryAction::Retry),
            "Fallback" => Some(RecoveryAction::Fallback),
            "ManualIntervention" => Some(RecoveryAction::ManualIntervention),
            "Abort" => Some(RecoveryAction::Abort),
            _ => None,
        })
        .unwrap_or(RecoveryAction::Abort)
}

use wf_common::error_chain::ErrorPattern;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_timeout_retryable() {
        let analysis = analyze_error(&AgentError::LlmError(LlmError::Timeout(30_000)));
        assert_eq!(analysis.kind, ErrorKind::Timeout);
        assert_eq!(analysis.error_type, ErrorType::Timeout);
        assert!(analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Retry);
    }

    #[test]
    fn test_llm_auth_not_retryable() {
        let analysis = analyze_error(&AgentError::LlmError(LlmError::AuthError(
            "bad key".to_string(),
        )));
        assert_eq!(analysis.kind, ErrorKind::AuthError);
        assert!(!analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::ManualIntervention);
    }

    #[test]
    fn test_tool_timeout_retryable() {
        let analysis = analyze_error(&AgentError::ToolError(ToolError::Timeout {
            tool_id: "read_file".to_string(),
            timeout_ms: 30,
        }));
        assert_eq!(analysis.kind, ErrorKind::Timeout);
        assert!(analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Retry);
    }

    #[test]
    fn test_tool_not_found_abort() {
        let analysis = analyze_error(&AgentError::ToolError(ToolError::NotFound(
            "missing".to_string(),
        )));
        assert_eq!(analysis.kind, ErrorKind::NotFound);
        assert!(!analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Abort);
    }

    #[test]
    fn test_state_error_abort() {
        let analysis = analyze_error(&AgentError::StateError("corrupt".to_string()));
        assert_eq!(analysis.kind, ErrorKind::StateManagement);
        assert!(!analysis.retryable);
    }

    #[test]
    fn test_hook_error_manual() {
        let analysis = analyze_error(&AgentError::HookError("hook failed".to_string()));
        assert_eq!(analysis.kind, ErrorKind::EventSystem);
        assert_eq!(analysis.recovery_action, RecoveryAction::ManualIntervention);
    }

    #[test]
    fn test_checkpoint_error() {
        let analysis = analyze_error(&AgentError::CheckpointError(
            wf_checkpoint::error::CheckpointError::NotFound {
                id: "x".to_string(),
            },
        ));
        assert_eq!(analysis.kind, ErrorKind::AgentCheckpoint);
    }

    #[test]
    fn test_to_error_record() {
        let analysis = analyze_error(&AgentError::LlmError(LlmError::Timeout(30_000)));
        let record = analysis.to_error_record("exec-1", Some("node-a".to_string()));
        assert_eq!(record.execution_id, "exec-1");
        assert_eq!(record.node_id.as_deref(), Some("node-a"));
        assert!(record.is_recoverable);
        assert!(matches!(
            record.recovery_action,
            Some(RecoveryAction::Retry)
        ));
        assert!(matches!(record.error_type, Some(ErrorType::Timeout)));
    }
}
