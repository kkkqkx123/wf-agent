//! Recovery proposal generation for workflow errors: heuristics for action
//! selection, likelihood estimation, recovery steps and time-to-recover.

use wf_common::error_chain::ErrorRecord;
use wf_types::enums::ErrorSeverity;

use super::advanced::severity_of;

/// Heuristic recovery action for a record without an explicit one.
pub(super) fn suggest_action(record: &ErrorRecord) -> String {
    if let Some(action) = &record.recovery_action {
        return crate::analysis::error_common::action_name(action);
    }
    match record.error_type {
        Some(wf_types::ErrorType::ToolError) => {
            if record.is_recoverable {
                "retry"
            } else {
                "fallback"
            }
        }
        Some(wf_types::ErrorType::Timeout) => "retry",
        Some(wf_types::ErrorType::Validation) => {
            if record.is_recoverable {
                "skip"
            } else {
                "fallback"
            }
        }
        _ => {
            if record.is_recoverable {
                "retry"
            } else {
                "abort"
            }
        }
    }
    .to_string()
}

/// Estimated recovery likelihood in percent for an error + action pair.
pub(super) fn estimate_likelihood(record: &ErrorRecord, action: &str) -> f64 {
    let mut likelihood: f64 = 50.0;
    if record.is_recoverable {
        likelihood += 30.0;
    }
    if severity_of(record) == ErrorSeverity::Warning {
        likelihood += 20.0;
    }
    match action {
        "retry" => likelihood += 15.0,
        "fallback" => likelihood += 10.0,
        "skip" => likelihood += 20.0,
        _ => {}
    }
    likelihood.clamp(0.0, 100.0)
}

/// Human-readable recovery steps for an action.
pub(super) fn recovery_steps(action: &str) -> Vec<String> {
    match action {
        "retry" => vec![
            "Wait a moment".to_string(),
            "Retry the failed operation".to_string(),
            "If still failing, escalate to manual intervention".to_string(),
        ],
        "fallback" => vec![
            "Check if a fallback implementation is available".to_string(),
            "Switch to the fallback implementation".to_string(),
            "Continue execution with the fallback".to_string(),
        ],
        "skip" => vec![
            "Mark the operation as skipped".to_string(),
            "Continue with the next operation".to_string(),
        ],
        _ => vec![
            "Log detailed error information".to_string(),
            "Clean up active resources".to_string(),
            "Gracefully stop the workflow execution".to_string(),
        ],
    }
}

/// Rough estimated time to recover in milliseconds per action.
pub(super) fn estimate_recovery_time(action: &str) -> Option<i64> {
    match action {
        "retry" => Some(1100),
        "fallback" => Some(600),
        "skip" => Some(200),
        _ => Some(0),
    }
}
