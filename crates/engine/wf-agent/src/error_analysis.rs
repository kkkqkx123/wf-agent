use std::collections::HashMap;

use wf_common::error_chain::{ErrorPattern, ErrorRecord};
use wf_execution_shared::error::ExecutionSharedError;
use wf_llm::error::LlmError;
use wf_tools::error::ToolError;
use wf_types::errors::{ErrorKind, ErrorType, RecoveryAction};

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
}

impl ErrorAnalysis {
    /// Build a root ErrorRecord for persistence into entity state /
    /// snapshots. Callers that need chain context (caused_by, parent links)
    /// fill those fields on the returned record.
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
            caused_by: None,
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

fn http_status_retryable(status: u16) -> bool {
    matches!(status, 429 | 500..=599)
}

/// 429/5xx failures keep their own `ErrorType` instead of masquerading as
/// `Timeout`, so aggregate pattern analysis does not count throttling and
/// upstream outages as plain timeouts. `plain` is the source's own type used
/// for the remaining statuses.
fn http_error_type(status: u16, plain: ErrorType) -> ErrorType {
    match status {
        429 => ErrorType::RateLimited,
        500..=599 => ErrorType::ServiceUnavailable,
        _ => plain,
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
            let retryable = http_status_retryable(*status);
            (
                kind,
                http_error_type(*status, ErrorType::ToolError),
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
                let status = s.as_u16();
                let kind = http_status_to_kind(status);
                let retryable = http_status_retryable(status);
                (
                    kind,
                    http_error_type(status, ErrorType::ToolError),
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
        ToolError::Cancelled { .. } => (
            ErrorKind::Execution,
            ErrorType::Interruption,
            false,
            RecoveryAction::Abort,
        ),
        // MCP and IO failures are usually transient (connection blips,
        // temporary unavailability): a conservative Retry instead of
        // dropping them, bounded by the retry budget at the policy layer.
        ToolError::McpError(_) | ToolError::Io(_) => (
            ErrorKind::Network,
            ErrorType::ToolError,
            true,
            RecoveryAction::Retry,
        ),
        // Remaining variants (ExecutionFailed, RetryExhausted, Serialization,
        // CallbackNotRegistered, Internal, ExecutionError) are terminal
        // failures; `RetryExhausted` has already spent its upstream budget.
        _ => (
            ErrorKind::Tool,
            ErrorType::ToolError,
            false,
            RecoveryAction::Abort,
        ),
    };
    debug_assert!(
        !(retryable && recovery_action == RecoveryAction::ManualIntervention),
        "retryable is the single decision bit: a retryable error must never settle on ManualIntervention"
    );
    ErrorAnalysis {
        kind,
        error_type,
        retryable,
        recovery_action,
        message: e.to_string(),
    }
}

pub fn llm_error_analysis(e: &LlmError) -> ErrorAnalysis {
    let (kind, error_type, retryable, recovery_action) = match e {
        // Payload overflow: the retry loop publishes a forced compression
        // before this analysis runs, so one retry lets compression catch up.
        // This is a resource budget failure, not a transient network fault.
        _ if e.is_context_length_exceeded() => (
            ErrorKind::Resource,
            ErrorType::LlmError,
            true,
            RecoveryAction::Retry,
        ),
        LlmError::Timeout(_) => (
            ErrorKind::Timeout,
            ErrorType::Timeout,
            true,
            RecoveryAction::Retry,
        ),
        LlmError::HttpError(h) => match h.status() {
            Some(s) => {
                let status = s.as_u16();
                let kind = http_status_to_kind(status);
                let retryable = http_status_retryable(status);
                (
                    kind,
                    http_error_type(status, ErrorType::LlmError),
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
                e.is_retryable(),
                if e.is_retryable() {
                    RecoveryAction::Retry
                } else {
                    RecoveryAction::ManualIntervention
                },
            ),
        },
        // Provider / stream transport failures defer retryability to the
        // wf-llm classification (5xx / 429 / connect resets are transient).
        LlmError::ProviderError(_) | LlmError::StreamError(_) => (
            ErrorKind::Network,
            ErrorType::LlmError,
            e.is_retryable(),
            if e.is_retryable() {
                RecoveryAction::Retry
            } else {
                RecoveryAction::ManualIntervention
            },
        ),
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
        // Configuration, codec, serialization and malformed-response errors
        // are deterministic failures; retrying reproduces the same outcome.
        _ => {
            let retryable = e.is_retryable();
            (
                ErrorKind::General,
                ErrorType::LlmError,
                retryable,
                if retryable {
                    RecoveryAction::Retry
                } else {
                    RecoveryAction::ManualIntervention
                },
            )
        }
    };
    debug_assert!(
        !(retryable && recovery_action == RecoveryAction::ManualIntervention),
        "retryable is the single decision bit: a retryable error must never settle on ManualIntervention"
    );
    ErrorAnalysis {
        kind,
        error_type,
        retryable,
        recovery_action,
        message: e.to_string(),
    }
}

pub fn shared_error_analysis(e: &ExecutionSharedError) -> ErrorAnalysis {
    // Interruption/timeout/category-tagged failures read through the single
    // `NodeErrorCategory` projection in wf-types, so records and workflow
    // routing can never disagree by maintaining two hand-written mappings.
    let (kind, error_type, recovery_action) = match e {
        ExecutionSharedError::StateError(_) => (
            ErrorKind::StateManagement,
            ErrorType::Internal,
            RecoveryAction::Abort,
        ),
        ExecutionSharedError::InterruptionError(_) => (
            ErrorKind::Execution,
            ErrorType::Interruption,
            RecoveryAction::Abort,
        ),
        ExecutionSharedError::TimeoutError(_) => (
            ErrorKind::Timeout,
            ErrorType::Timeout,
            RecoveryAction::Abort,
        ),
        ExecutionSharedError::VariableError(_) => (
            ErrorKind::Validation,
            ErrorType::Validation,
            RecoveryAction::Abort,
        ),
        ExecutionSharedError::NodeFailure { category, .. } => (
            category.error_kind(),
            category.error_type(),
            RecoveryAction::Abort,
        ),
        ExecutionSharedError::ToolError(te) => {
            let analysis = tool_error_analysis(te);
            return analysis;
        }
        _ => (
            ErrorKind::Execution,
            ErrorType::Internal,
            RecoveryAction::Abort,
        ),
    };
    ErrorAnalysis {
        kind,
        error_type,
        retryable: false,
        recovery_action,
        message: e.to_string(),
    }
}

/// Classify an agent error into a structured analysis covering all AgentError
/// branches.
pub fn analyze_error(e: &AgentError) -> ErrorAnalysis {
    match e {
        AgentError::IllegalStateTransition(_) => ErrorAnalysis {
            kind: ErrorKind::StateManagement,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        // Deterministic caller/config misuse: the same request reproduces the
        // same rejection, so only the request itself can change.
        AgentError::Validation(_) => ErrorAnalysis {
            kind: ErrorKind::Validation,
            error_type: ErrorType::Validation,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        // Whole-execution timeouts end the run; a retry re-pays the entire
        // iteration budget, so the engine does not offer one.
        AgentError::ExecutionTimeout(_) => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Timeout,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        AgentError::Cancelled(_) => ErrorAnalysis {
            kind: ErrorKind::Execution,
            error_type: ErrorType::Interruption,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
        // A full gate is transient saturation: the same admission succeeds
        // once in-flight executions drain.
        AgentError::ConcurrencySaturated(_) => ErrorAnalysis {
            kind: ErrorKind::Resource,
            error_type: ErrorType::Internal,
            retryable: true,
            recovery_action: RecoveryAction::Retry,
            message: e.to_string(),
        },
        AgentError::HierarchyLimitReached(_) => ErrorAnalysis {
            kind: ErrorKind::Resource,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::ManualIntervention,
            message: e.to_string(),
        },
        AgentError::LlmError(le) => llm_error_analysis(le),
        AgentError::CheckpointError(_) => ErrorAnalysis {
            kind: ErrorKind::AgentCheckpoint,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::ManualIntervention,
            message: e.to_string(),
        },
        AgentError::SharedError(se) => shared_error_analysis(se),
        AgentError::Internal(_) => ErrorAnalysis {
            kind: ErrorKind::General,
            error_type: ErrorType::Internal,
            retryable: false,
            recovery_action: RecoveryAction::Abort,
            message: e.to_string(),
        },
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

/// Count record entries by a key extractor.
fn count_by<K, F>(records: &[ErrorRecord], extract: F) -> HashMap<K, usize>
where
    K: Clone + Eq + std::hash::Hash,
    F: Fn(&ErrorRecord) -> Option<&K>,
{
    let mut counts: HashMap<K, usize> = HashMap::new();
    for record in records {
        if let Some(key) = extract(record) {
            *counts.entry(key.clone()).or_insert(0) += 1;
        }
    }
    counts
}

/// Highest-count entry of a typed aggregate.
fn most_common<K>(counts: &HashMap<K, usize>) -> Option<K>
where
    K: Clone + Eq + std::hash::Hash,
{
    counts.iter().max_by_key(|(_, count)| **count).map(|(k, _)| k.clone())
}

/// Analyze error patterns from a list of error records.
pub fn analyze_error_pattern(records: &[ErrorRecord]) -> ErrorPattern {
    let type_dist = count_by(records, |r| r.error_type.as_ref());
    let recovery_count = count_by(records, |r| r.recovery_action.as_ref());

    let mut affected_nodes: Vec<String> = Vec::new();
    let mut has_recoverable = false;
    for record in records {
        if let Some(ref node_id) = record.node_id {
            if !affected_nodes.contains(node_id) {
                affected_nodes.push(node_id.clone());
            }
        }
        if record.is_recoverable {
            has_recoverable = true;
        }
    }

    ErrorPattern {
        total_errors: records.len(),
        type_distribution: type_dist
            .iter()
            .map(|(t, c)| (format!("{:?}", t), *c))
            .collect(),
        affected_nodes,
        most_common_type: most_common(&type_dist),
        has_recoverable,
        recovery_action_count: recovery_count
            .iter()
            .map(|(a, c)| (format!("{:?}", a), *c))
            .collect(),
    }
}

/// Get the recommended recovery action based on error pattern analysis.
pub fn get_recommended_recovery_action(records: &[ErrorRecord]) -> RecoveryAction {
    if !records.iter().any(|r| r.is_recoverable) {
        return RecoveryAction::Abort;
    }
    let counts = count_by(records, |r| r.recovery_action.as_ref());
    most_common(&counts).unwrap_or(RecoveryAction::Abort)
}

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
        let analysis = tool_error_analysis(&ToolError::Timeout {
            tool_id: "read_file".to_string(),
            timeout_ms: 30,
        });
        assert_eq!(analysis.kind, ErrorKind::Timeout);
        assert!(analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Retry);
    }

    #[test]
    fn test_tool_not_found_abort() {
        let analysis = tool_error_analysis(&ToolError::NotFound("missing".to_string()));
        assert_eq!(analysis.kind, ErrorKind::NotFound);
        assert!(!analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Abort);
    }

    #[test]
    fn test_tool_rate_limited_keeps_own_error_type() {
        let analysis = tool_error_analysis(&ToolError::RestError {
            url: "https://api.example.com".to_string(),
            status: 429,
        });
        assert_eq!(analysis.kind, ErrorKind::RateLimited);
        assert_eq!(analysis.error_type, ErrorType::RateLimited);
        assert!(analysis.retryable);
    }

    #[test]
    fn test_validation_abort() {
        let analysis = analyze_error(&AgentError::Validation("bad cap".to_string()));
        assert_eq!(analysis.kind, ErrorKind::Validation);
        assert!(!analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Abort);
    }

    #[test]
    fn test_concurrency_saturated_is_retryable() {
        let analysis = analyze_error(&AgentError::ConcurrencySaturated("gate full".to_string()));
        assert_eq!(analysis.kind, ErrorKind::Resource);
        assert!(analysis.retryable);
        assert_eq!(analysis.recovery_action, RecoveryAction::Retry);
    }

    #[test]
    fn test_shared_interruption_matches_routing_reading() {
        use wf_types::workflow::error_branch::NodeErrorCategory;
        let analysis = shared_error_analysis(&ExecutionSharedError::InterruptionError(
            "stopped".to_string(),
        ));
        assert_eq!(analysis.error_type, ErrorType::Interruption);
        assert_eq!(
            NodeErrorCategory::from_error_type(&analysis.error_type),
            NodeErrorCategory::CancelledInterrupted
        );
        let analysis =
            shared_error_analysis(&ExecutionSharedError::TimeoutError("slow".to_string()));
        assert_eq!(analysis.error_type, ErrorType::Timeout);
        assert_eq!(
            NodeErrorCategory::from_error_type(&analysis.error_type),
            NodeErrorCategory::TransportTimeout
        );
        // A category-tagged failure projects back through the same mapping.
        let analysis = shared_error_analysis(&ExecutionSharedError::NodeFailure {
            node_id: "n".to_string(),
            category: NodeErrorCategory::Resource,
            detail: "rate limited".to_string(),
        });
        assert_eq!(analysis.error_type, ErrorType::ServiceUnavailable);
        assert_eq!(analysis.kind, ErrorKind::Resource);
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
