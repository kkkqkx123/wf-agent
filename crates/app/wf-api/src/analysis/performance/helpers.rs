use std::collections::BTreeMap;

use crate::infra::util::round2;
use wf_types::enums::BottleneckSeverity;

use super::types::*;

/// Average of a duration slice, or 0 when empty.
pub(super) fn avg(values: &[i64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<i64>() as f64 / values.len() as f64
    }
}

/// Build a performance summary from node timing entries.
pub(super) fn build_summary(
    node_executions: &[NodeTimelineEntry],
    total_duration_ms: i64,
) -> PerformanceSummaryView {
    let durations: Vec<i64> = node_executions.iter().map(|e| e.duration_ms).collect();
    let avg_node_duration = if durations.is_empty() {
        0
    } else {
        durations.iter().sum::<i64>() / durations.len() as i64
    };
    let min_node_duration = durations.iter().copied().min().unwrap_or(0);
    let max_node_duration = durations.iter().copied().max().unwrap_or(0);
    let success_count = node_executions.iter().filter(|e| e.success).count();
    let success_rate = if node_executions.is_empty() {
        0.0
    } else {
        success_count as f64 / node_executions.len() as f64
    };
    let operations_per_second = if total_duration_ms > 0 {
        node_executions.len() as f64 / (total_duration_ms as f64 / 1000.0)
    } else {
        0.0
    };

    let mut recommendations = Vec::new();
    if max_node_duration > SLOW_NODE_THRESHOLD_MS {
        recommendations.push(format!(
            "slowest node took {max_node_duration}ms; consider parallelizing or optimizing it"
        ));
    }
    if success_rate < 1.0 {
        recommendations
            .push("some node executions failed; review retry and error handling".to_string());
    }
    if operations_per_second > 0.0 && operations_per_second < 1.0 {
        recommendations
            .push("low operation throughput; consider increasing parallelism".to_string());
    }

    PerformanceSummaryView {
        avg_node_duration,
        min_node_duration,
        max_node_duration,
        success_rate: round2(success_rate),
        operations_per_second: round2(operations_per_second),
        recommendations,
    }
}

/// Identify and rank the bottlenecks of an execution.
pub(super) fn build_bottleneck_views(
    node_executions: &[NodeTimelineEntry],
    total_duration_ms: i64,
) -> Vec<PerformanceBottleneckView> {
    // Fall back to the summed node durations when the execution span is not
    // recorded (e.g. state boundaries collapse within a millisecond).
    let denominator = if total_duration_ms > 0 {
        total_duration_ms
    } else {
        node_executions
            .iter()
            .map(|e| e.duration_ms)
            .sum::<i64>()
            .max(1)
    };
    let mut bottlenecks: Vec<PerformanceBottleneckView> = node_executions
        .iter()
        .map(|entry| {
            let percentage = entry.duration_ms as f64 / denominator as f64;
            let severity = if percentage >= BOTTLENECK_HIGH_RATIO {
                BottleneckSeverity::High
            } else if percentage >= BOTTLENECK_MEDIUM_RATIO {
                BottleneckSeverity::Medium
            } else {
                BottleneckSeverity::Low
            };
            PerformanceBottleneckView {
                r#type: "node".to_string(),
                location: format!("{} ({})", entry.node_name, entry.node_id),
                duration: entry.duration_ms,
                percentage: round3(percentage),
                severity,
            }
        })
        .collect();
    bottlenecks.sort_by(|a, b| {
        b.duration
            .cmp(&a.duration)
            .then_with(|| a.location.cmp(&b.location))
    });
    bottlenecks.truncate(MAX_BOTTLENECKS);
    bottlenecks
}

/// Classify a total duration into a performance tier.
pub(super) fn classify_performance(total_duration_ms: i64) -> PerformanceTier {
    if total_duration_ms <= FAST_TIER_MS {
        PerformanceTier::Fast
    } else if total_duration_ms <= NORMAL_TIER_MS {
        PerformanceTier::Normal
    } else {
        PerformanceTier::Slow
    }
}

pub(super) fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

pub(super) fn build_profile(
    execution_id: &str,
    workflow_id: Option<String>,
    status: String,
    total_duration_ms: i64,
    node_count: u32,
    error_count: u32,
    timeline: Vec<NodeTimelineEntry>,
) -> ExecutionPerformanceProfile {
    let mut bottlenecks = timeline.clone();
    bottlenecks.sort_by(|a, b| {
        b.duration_ms
            .cmp(&a.duration_ms)
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    bottlenecks.truncate(PROFILE_BOTTLENECK_COUNT);

    let mut time_by_node_type = BTreeMap::new();
    for entry in &timeline {
        *time_by_node_type
            .entry(entry.node_type.clone())
            .or_insert(0) += entry.duration_ms;
    }

    ExecutionPerformanceProfile {
        execution_id: execution_id.to_string(),
        workflow_id,
        status,
        total_duration_ms,
        node_count,
        error_count,
        timeline,
        bottlenecks,
        time_by_node_type,
    }
}


