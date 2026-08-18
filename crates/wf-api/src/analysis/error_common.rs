//! Shared error-analysis helpers used by both the workflow and the agent
//! error-analysis surfaces (`analysis::error_analysis` and
//! `agent::agent_error_analysis`).

use wf_common::error_chain::ErrorRecord;
use wf_types::errors::RecoveryAction;

use crate::agent::agent_error_analysis::ExecutionErrorRecord;

/// Map an `ErrorRecord` onto the shared serializable view.
pub fn record_view(record: &ErrorRecord) -> ExecutionErrorRecord {
    ExecutionErrorRecord {
        id: record.id.clone(),
        execution_id: record.execution_id.clone(),
        error: record.error.clone(),
        error_type: record.error_type.as_ref().map(|t| format!("{t:?}")),
        timestamp: record.timestamp,
        node_id: record.node_id.clone(),
        parent_error_id: record.parent_error_id.clone(),
        error_chain: record.error_chain.clone(),
        root_cause_id: record.root_cause_id.clone(),
        caused_by: record.caused_by.as_ref().map(|c| c.reason.clone()),
        is_recoverable: record.is_recoverable,
        recovery_action: record.recovery_action.as_ref().map(action_name),
    }
}

/// Stable serialized name of a recovery action.
pub fn action_name(action: &RecoveryAction) -> String {
    match action {
        RecoveryAction::Retry => "retry".to_string(),
        RecoveryAction::Fallback => "fallback".to_string(),
        RecoveryAction::ManualIntervention => "manual_intervention".to_string(),
        RecoveryAction::Abort => "abort".to_string(),
    }
}

/// Normalize an error message for similarity clustering: lowercase, collapse
/// whitespace and strip common retry/attempt suffixes.
pub fn normalize_message(message: &str) -> String {
    let trimmed = message.trim().to_lowercase();
    let trimmed = trimmed
        .strip_suffix(")")
        .map(|s| s.rsplit_once("(").map(|(head, _)| head).unwrap_or(s))
        .unwrap_or(&trimmed);
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Minimal `ErrorRecord` for persisted executions that carry only a plain
/// error text (no structured error chain).
pub fn minimal_record(execution_id: &str, error: &str, node_id: Option<String>) -> ErrorRecord {
    ErrorRecord {
        id: wf_common::generate_id(),
        execution_id: execution_id.to_string(),
        error: error.to_string(),
        error_type: None,
        timestamp: wf_common::now(),
        node_id,
        parent_error_id: None,
        error_chain: Vec::new(),
        root_cause_id: String::new(),
        caused_by: None,
        is_recoverable: false,
        recovery_action: None,
    }
}
