//! Advanced-analysis helpers: severity buckets, temporal patterns, error
//! trends and severity ranking.

use wf_common::error_chain::ErrorRecord;
use wf_types::enums::{ErrorSeverity, ErrorTrend};
use wf_types::errors::RecoveryAction;

/// Coarse severity bucket used by the stats view.
pub(super) fn severity_of(record: &ErrorRecord) -> ErrorSeverity {
    match &record.recovery_action {
        Some(RecoveryAction::Retry) => ErrorSeverity::Warning,
        Some(RecoveryAction::ManualIntervention) | Some(RecoveryAction::Abort) => {
            ErrorSeverity::Critical
        }
        None => {
            if record.is_recoverable {
                ErrorSeverity::Warning
            } else {
                ErrorSeverity::Critical
            }
        }
    }
}

/// Temporal pattern of error occurrence.
pub(super) fn analyze_temporal_pattern(sorted: &[ErrorRecord]) -> String {
    if sorted.len() < 3 {
        return "none".to_string();
    }
    let intervals: Vec<i64> = sorted
        .windows(2)
        .map(|pair| pair[1].timestamp - pair[0].timestamp)
        .collect();
    if intervals.len() < 2 {
        return "none".to_string();
    }
    let recent = &intervals[intervals.len().saturating_sub(3)..];
    let early = &intervals[..intervals.len().min(3)];
    let recent_avg = recent.iter().sum::<i64>() as f64 / recent.len().max(1) as f64;
    let early_avg = early.iter().sum::<i64>() as f64 / early.len().max(1) as f64;
    if recent_avg > 0.0 && early_avg > 0.0 {
        if recent_avg < early_avg * 0.7 {
            return "accelerating".to_string();
        }
        if recent_avg > early_avg * 1.3 {
            return "decelerating".to_string();
        }
    }
    "steady".to_string()
}

/// Error trend direction by comparing the first and second half.
pub(super) fn analyze_error_trend(sorted: &[ErrorRecord]) -> ErrorTrend {
    if sorted.len() < 2 {
        return ErrorTrend::Stable;
    }
    let mid = sorted.len() / 2;
    let first = sorted.len().saturating_sub(mid);
    let second = mid;
    let ratio = if first > 0 {
        second as f64 / first as f64
    } else {
        1.0
    };
    if ratio > 1.3 {
        ErrorTrend::Increasing
    } else if ratio < 0.7 {
        ErrorTrend::Decreasing
    } else {
        ErrorTrend::Stable
    }
}

/// Rank order of a severity label (lower = less severe).
pub(super) fn severity_rank(severity: ErrorSeverity) -> u8 {
    match severity {
        ErrorSeverity::Warning => 0,
        ErrorSeverity::Critical => 2,
    }
}
