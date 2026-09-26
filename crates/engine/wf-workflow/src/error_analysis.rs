use wf_agent::error_analysis::{shared_error_analysis, ErrorAnalysis};
use wf_common::error_chain::ErrorRecord;
use wf_types::errors::{ErrorKind, ErrorType, RecoveryAction};
use wf_types::workflow::error_branch::NodeErrorCategory;

use crate::error::WorkflowError;

/// Project a nested agent-loop failure into a typed `NodeFailure` so both the
/// sync and the streamed agent node path route by category instead of
/// collapsing into an untyped handler error. The category comes from the
/// single `wf_types` mapping shared with the agent-side analysis.
pub fn agent_failure_node_failure(
    node_id: &str,
    error_type: ErrorType,
    detail: String,
) -> WorkflowError {
    WorkflowError::NodeFailure {
        node_id: node_id.to_string(),
        category: NodeErrorCategory::from_error_type(&error_type),
        detail,
    }
}

/// Classify a workflow error into a structured analysis, reusing the
/// agent-side classifiers for shared error types so workflow and agent
/// executions produce comparable error records.
pub fn analyze_workflow_error(e: &WorkflowError) -> ErrorAnalysis {
    match e {
        WorkflowError::CoordinatorError(_) => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        WorkflowError::ExecutionTimeout(_) => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Timeout,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        WorkflowError::GraphError(_) | WorkflowError::VariableError(_) => ErrorAnalysis {
            kind: ErrorKind::Validation,
            error_type: ErrorType::Validation,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        WorkflowError::HandlerNotFound { .. } => ErrorAnalysis {
            kind: ErrorKind::NotFound,
            error_type: ErrorType::Validation,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        WorkflowError::NodeExecutionFailed { .. } => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        // Terminal node failure: transient retries are spent inside the
        // handler, so by the time this reaches the coordinator nothing in
        // the engine re-runs it. Claiming `Retry` here would advertise a
        // recovery the executor never performs. The recorded type and kind
        // follow the routing category, so a terminal timeout or quota failure
        // never persists as a generic internal error.
        WorkflowError::NodeFailure { category, .. } => ErrorAnalysis {
            kind: category.error_kind(),
            error_type: category.error_type(),
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        WorkflowError::ForkJoinError(_)
        | WorkflowError::SubgraphError(_)
        | WorkflowError::TriggerError(_) => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::ManualIntervention,
            message: e.to_string(),
        },
        WorkflowError::StateTransitionError(_) => ErrorAnalysis {
            kind: ErrorKind::StateManagement,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        // OperationError / LoopError sites are predominantly config and
        // wiring misuse (missing registry, unregistered tool, bad node
        // settings); re-running cannot fix them, so the honest advice is
        // manual intervention, not a retry.
        WorkflowError::OperationError(_) | WorkflowError::LoopError(_) => ErrorAnalysis {
            kind: ErrorKind::General,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::ManualIntervention,
            message: e.to_string(),
        },
        WorkflowError::ConfigError { .. } => ErrorAnalysis {
            kind: ErrorKind::Validation,
            error_type: ErrorType::Validation,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        WorkflowError::SharedError(se) => shared_error_analysis(se),
        WorkflowError::Internal(_) => ErrorAnalysis {
            kind: ErrorKind::General,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
    }
}

/// Build a persisted ErrorRecord for one failed node attempt. Chain links are
/// filled by the caller from prior records. Engine-level retries are invisible
/// here (they are spent inside handlers and never re-run by the coordinator),
/// so the record carries no retry context.
pub fn workflow_error_record(e: &WorkflowError, execution_id: &str, node_id: &str) -> ErrorRecord {
    let analysis = analyze_workflow_error(e);
    analysis.to_error_record(execution_id, Some(node_id.to_string()))
}

/// Aggregate error information across an execution's error records: total,
/// affected nodes, most common error type, presence of recoverable errors and
/// the most frequently recommended recovery action.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WorkflowErrorPattern {
    pub total_errors: usize,
    pub affected_nodes: Vec<String>,
    pub most_common_type: Option<ErrorType>,
    pub has_recoverable: bool,
    pub recovery_action_count: std::collections::HashMap<String, usize>,
}

/// Compute an aggregate error pattern from the persisted error records of an
/// execution, so recovery can be recommended at a workflow granularity.
pub fn analyze_workflow_error_pattern(records: &[ErrorRecord]) -> WorkflowErrorPattern {
    let mut affected_nodes: Vec<String> = Vec::new();
    let mut type_dist: std::collections::HashMap<ErrorType, usize> =
        std::collections::HashMap::new();
    let mut recovery_action_count: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut has_recoverable = false;

    for record in records {
        if let Some(ref node_id) = record.node_id {
            if !affected_nodes.contains(node_id) {
                affected_nodes.push(node_id.clone());
            }
        }
        if let Some(ref error_type) = record.error_type {
            *type_dist.entry(error_type.clone()).or_insert(0) += 1;
        }
        if record.is_recoverable {
            has_recoverable = true;
        }
        if let Some(ref action) = record.recovery_action {
            *recovery_action_count
                .entry(format!("{:?}", action))
                .or_insert(0) += 1;
        }
    }

    let most_common_type = type_dist
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(error_type, _)| error_type.clone());

    WorkflowErrorPattern {
        total_errors: records.len(),
        affected_nodes,
        most_common_type,
        has_recoverable,
        recovery_action_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::errors::RecoveryAction;

    #[test]
    fn node_execution_failed_is_terminal_abort() {
        let analysis = analyze_workflow_error(&WorkflowError::NodeExecutionFailed {
            node_id: "n1".to_string(),
            reason: "boom".to_string(),
        });
        assert_eq!(analysis.kind, ErrorKind::Execution);
        assert!(!analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Abort);
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
    fn operation_error_is_not_retryable() {
        let analysis =
            analyze_workflow_error(&WorkflowError::OperationError("bad wiring".to_string()));
        assert_eq!(analysis.kind, ErrorKind::General);
        assert!(!analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::ManualIntervention);
    }

    #[test]
    fn state_transition_error_aborts() {
        let analysis =
            analyze_workflow_error(&WorkflowError::StateTransitionError("corrupt".to_string()));
        assert_eq!(analysis.kind, ErrorKind::StateManagement);
        assert!(!analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Abort);
    }

    #[test]
    fn shared_tool_timeout_stays_retryable() {
        // A tool timeout is genuinely retryable, so the record stays
        // marked recoverable through the shared wrapper.
        let record = workflow_error_record(
            &WorkflowError::SharedError(
                wf_execution_shared::error::ExecutionSharedError::ToolError(
                    wf_tools::error::ToolError::Timeout {
                        tool_id: "t".to_string(),
                        timeout_ms: 1000,
                    },
                ),
            ),
            "exec-1",
            "n-7",
        );
        assert_eq!(record.node_id.as_deref(), Some("n-7"));
        assert_eq!(record.execution_id, "exec-1");
        assert!(record.is_recoverable);
        assert!(record.caused_by.is_none());
    }

    #[test]
    fn node_failure_records_follow_the_routing_category() {
        let timeout = analyze_workflow_error(&WorkflowError::NodeFailure {
            node_id: "x".to_string(),
            category: NodeErrorCategory::TransportTimeout,
            detail: "child wall clock exceeded".to_string(),
        });
        assert_eq!(timeout.error_type, ErrorType::Timeout);
        assert_eq!(timeout.kind, ErrorKind::Timeout);

        let cancelled = analyze_workflow_error(&WorkflowError::NodeFailure {
            node_id: "x".to_string(),
            category: NodeErrorCategory::CancelledInterrupted,
            detail: "stopped".to_string(),
        });
        assert_eq!(cancelled.error_type, ErrorType::Interruption);

        let resource = analyze_workflow_error(&WorkflowError::NodeFailure {
            node_id: "x".to_string(),
            category: NodeErrorCategory::Resource,
            detail: "rate limited".to_string(),
        });
        assert_eq!(resource.kind, ErrorKind::Resource);
        assert_eq!(resource.error_type, ErrorType::ServiceUnavailable);
    }

    #[test]
    fn agent_terminal_errors_route_by_category_through_the_node_boundary() {
        use crate::error_branch::classify_error;
        // The exact chain the agent-node handler runs: AgentError analysis →
        // typed NodeFailure → coordinator routing. A timeout and a
        // cancellation must keep transport semantics, never collapse to
        // `BusinessFailure`.
        for (agent_error, expected) in [
            (
                wf_agent::error::AgentError::ExecutionTimeout("budget".to_string()),
                NodeErrorCategory::TransportTimeout,
            ),
            (
                wf_agent::error::AgentError::Cancelled("stopped".to_string()),
                NodeErrorCategory::CancelledInterrupted,
            ),
        ] {
            let analysis = wf_agent::error_analysis::analyze_error(&agent_error);
            let projected = agent_failure_node_failure(
                "agent-node",
                analysis.error_type.clone(),
                analysis.message,
            );
            assert_eq!(classify_error(&projected), expected, "{agent_error}");
        }
    }

    #[test]
    fn node_execution_failed_keeps_its_category_across_the_shared_boundary() {
        let shared: wf_execution_shared::error::ExecutionSharedError =
            WorkflowError::NodeExecutionFailed {
                node_id: "n1".to_string(),
                reason: "script exited with code 1".to_string(),
            }
            .into();
        assert!(matches!(
            shared,
            wf_execution_shared::error::ExecutionSharedError::NodeFailure {
                category: NodeErrorCategory::BusinessFailure,
                ..
            }
        ));
    }
}
