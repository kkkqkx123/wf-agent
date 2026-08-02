use wf_agent::error_analysis::{
    analyze_error, shared_error_analysis, tool_error_analysis, ErrorAnalysis,
};
use wf_common::error_chain::ErrorRecord;
use wf_types::errors::{ErrorCause, ErrorKind, ErrorType, RecoveryAction};

use crate::error::WorkflowError;

/// Classify a workflow error into a structured analysis, reusing the
/// agent-side classifiers for shared error types so workflow and agent
/// executions produce comparable error records.
pub fn analyze_workflow_error(e: &WorkflowError) -> ErrorAnalysis {
    match e {
        WorkflowError::EntityError(_) | WorkflowError::StateError(_) => ErrorAnalysis {
            kind: ErrorKind::StateManagement,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
        WorkflowError::CoordinatorError(_) => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
        WorkflowError::GraphError(_) | WorkflowError::VariableError(_) => ErrorAnalysis {
            kind: ErrorKind::Validation,
            error_type: ErrorType::Validation,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
        WorkflowError::HandlerNotFound { .. } => ErrorAnalysis {
            kind: ErrorKind::NotFound,
            error_type: ErrorType::Validation,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
        WorkflowError::NodeExecutionFailed { .. } => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Internal,
            retryable: true,
            recovery_action: RecoveryAction::Retry,
            message: e.to_string(),
            cause: None,
        },
        WorkflowError::ForkJoinError(_)
        | WorkflowError::SubgraphError(_)
        | WorkflowError::TriggerError(_) => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::ManualIntervention,
            message: e.to_string(),
            cause: None,
        },
        WorkflowError::StateTransitionError(_) => ErrorAnalysis {
            kind: ErrorKind::StateManagement,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
        WorkflowError::OperationError(_) => ErrorAnalysis {
            kind: ErrorKind::General,
            error_type: ErrorType::Internal,
            retryable: true,
            recovery_action: RecoveryAction::Retry,
            message: e.to_string(),
            cause: None,
        },
        WorkflowError::ToolError(te) => tool_error_analysis(te),
        WorkflowError::CoreError(_) | WorkflowError::Internal(_) => ErrorAnalysis {
            kind: ErrorKind::General,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
            cause: None,
        },
        WorkflowError::SharedError(se) => shared_error_analysis(se),
        WorkflowError::AgentError(ae) => analyze_error(ae),
    }
}

/// Build a persisted ErrorRecord for one failed node attempt. The retry
/// attempt index is attached to the cause so the record carries the retry
/// context; chain links are filled by the caller from prior records.
pub fn workflow_error_record(
    e: &WorkflowError,
    execution_id: &str,
    node_id: &str,
    retry_attempt: u32,
) -> ErrorRecord {
    let analysis = analyze_workflow_error(e);
    let mut record = analysis.to_error_record(execution_id, Some(node_id.to_string()));
    record.caused_by = Some(ErrorCause {
        reason: e.to_string(),
        handling_attempt: Some(format!("retry_{}", retry_attempt)),
    });
    if retry_attempt > 0 {
        record.error = format!("{} (retry attempt {})", e, retry_attempt);
    }
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::errors::RecoveryAction;

    #[test]
    fn node_execution_failed_is_retryable() {
        let analysis = analyze_workflow_error(&WorkflowError::NodeExecutionFailed {
            node_id: "n1".to_string(),
            reason: "boom".to_string(),
        });
        assert_eq!(analysis.kind, ErrorKind::Execution);
        assert!(analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Retry);
    }

    #[test]
    fn graph_error_is_validation() {
        let analysis =
            analyze_workflow_error(&WorkflowError::GraphError("no start node".to_string()));
        assert_eq!(analysis.kind, ErrorKind::Validation);
        assert!(!analysis.retryable);
    }

    #[test]
    fn handler_not_found_is_not_found() {
        let analysis = analyze_workflow_error(&WorkflowError::HandlerNotFound {
            node_type: "NOPE".to_string(),
        });
        assert_eq!(analysis.kind, ErrorKind::NotFound);
        assert_eq!(analysis.error_type, ErrorType::Validation);
    }

    #[test]
    fn operation_error_is_retryable_general() {
        let analysis =
            analyze_workflow_error(&WorkflowError::OperationError("transient".to_string()));
        assert_eq!(analysis.kind, ErrorKind::General);
        assert!(analysis.retryable);
    }

    #[test]
    fn state_error_aborts() {
        let analysis = analyze_workflow_error(&WorkflowError::StateError("corrupt".to_string()));
        assert_eq!(analysis.kind, ErrorKind::StateManagement);
        assert!(!analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Abort);
    }

    #[test]
    fn record_carries_retry_attempt() {
        let record = workflow_error_record(
            &WorkflowError::OperationError("transient".to_string()),
            "exec-1",
            "n-7",
            2,
        );
        assert_eq!(record.node_id.as_deref(), Some("n-7"));
        assert_eq!(record.execution_id, "exec-1");
        assert!(record.error.contains("retry attempt 2"));
        assert!(matches!(
            record.caused_by,
            Some(ref cause)
                if cause.handling_attempt.as_deref() == Some("retry_2")
        ));
        assert!(record.is_recoverable);
    }

    #[test]
    fn record_first_attempt_has_no_retry_suffix() {
        let record = workflow_error_record(
            &WorkflowError::NodeExecutionFailed {
                node_id: "n-7".to_string(),
                reason: "boom".to_string(),
            },
            "exec-1",
            "n-7",
            0,
        );
        assert!(!record.error.contains("retry attempt"));
    }
}
