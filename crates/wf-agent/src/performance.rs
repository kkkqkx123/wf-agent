//! Lightweight agent execution performance profiling.
//!
//! Pure functions over `AgentLoopStateSnapshot`; no persistence.
//! Duration classification, trend and bottleneck detection live in
//! `wf-execution-shared::types::performance` and are shared with workflow
//! analysis. The TS `PerformanceMetricsAPI` full version (per-iteration
//! token/cost breakdown, cross-execution comparison) is deliberately not
//! implemented: its data sources do not exist in Rust and the TS results were
//! unreliable (hardcoded counts, model-level aggregation).

use serde::{Deserialize, Serialize};

use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_execution_shared::types::performance::{
    analyze_trend, classify_bottleneck, classify_duration,
};

use crate::state::{AgentLoopStateSnapshot, ToolCallRecord};

pub use wf_execution_shared::types::performance::{
    BottleneckSeverity, DurationClass, PerformanceTrend,
};

const SLOWEST_LIMIT: usize = 10;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DurationDistribution {
    pub fast: u32,
    pub normal: u32,
    pub slow: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bottleneck {
    pub iteration: u32,
    pub duration_ms: i64,
    pub severity: BottleneckSeverity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlowIteration {
    pub iteration: u32,
    pub duration_ms: i64,
    pub tool_call_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IterationPerformance {
    pub iteration: u32,
    pub duration_ms: i64,
    pub tool_call_count: u32,
    pub tool_calls: Vec<ToolCallRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPerformanceProfile {
    pub status: ExecutionStatus,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub total_duration_ms: Option<i64>,
    pub total_iterations: u32,
    pub total_tool_calls: u32,
    pub avg_duration_ms: f64,
    pub duration_distribution: DurationDistribution,
    pub iterations: Vec<IterationPerformance>,
    pub bottlenecks: Vec<Bottleneck>,
    pub trend: PerformanceTrend,
    pub slowest_iterations: Vec<SlowIteration>,
}

impl Default for ExecutionPerformanceProfile {
    fn default() -> Self {
        Self {
            status: ExecutionStatus::Created,
            start_time: 0,
            end_time: None,
            total_duration_ms: None,
            total_iterations: 0,
            total_tool_calls: 0,
            avg_duration_ms: 0.0,
            duration_distribution: DurationDistribution::default(),
            iterations: Vec::new(),
            bottlenecks: Vec::new(),
            trend: PerformanceTrend::InsufficientData,
            slowest_iterations: Vec::new(),
        }
    }
}

/// Profile an agent execution from its state snapshot.
///
/// Only completed iterations (`end_time` set) contribute durations. Trend
/// needs at least 4 iterations; fewer yield `InsufficientData`.
pub fn analyze_performance(state: &AgentLoopStateSnapshot) -> ExecutionPerformanceProfile {
    let total_duration_ms = state.end_time.map(|end| end - state.start_time);

    let durations: Vec<(u32, i64, u32)> = state
        .iteration_history
        .iter()
        .filter_map(|record| {
            record.end_time.map(|end| {
                (
                    record.iteration,
                    end - record.start_time,
                    record.tool_call_count,
                )
            })
        })
        .collect();

    let total_iterations = durations.len() as u32;
    if total_iterations == 0 {
        return ExecutionPerformanceProfile {
            status: state.status.clone(),
            start_time: state.start_time,
            end_time: state.end_time,
            total_duration_ms,
            total_tool_calls: state.tool_call_count,
            ..ExecutionPerformanceProfile::default()
        };
    }

    let sum: i64 = durations.iter().map(|(_, duration, _)| *duration).sum();
    let avg = sum as f64 / total_iterations as f64;

    let mut distribution = DurationDistribution::default();
    let mut bottlenecks = Vec::new();
    let mut iterations = Vec::with_capacity(durations.len());
    let mut slowest: Vec<SlowIteration> = Vec::with_capacity(durations.len());

    for (iteration, duration, tool_call_count) in &durations {
        match classify_duration(*duration) {
            DurationClass::Fast => distribution.fast += 1,
            DurationClass::Normal => distribution.normal += 1,
            DurationClass::Slow => distribution.slow += 1,
        }

        if let Some(severity) = classify_bottleneck(*duration, avg) {
            bottlenecks.push(Bottleneck {
                iteration: *iteration,
                duration_ms: *duration,
                severity,
            });
        }

        let tool_calls = state
            .iteration_history
            .iter()
            .find(|r| r.iteration == *iteration)
            .map(|r| r.tool_calls.clone())
            .unwrap_or_default();
        iterations.push(IterationPerformance {
            iteration: *iteration,
            duration_ms: *duration,
            tool_call_count: *tool_call_count,
            tool_calls,
        });

        slowest.push(SlowIteration {
            iteration: *iteration,
            duration_ms: *duration,
            tool_call_count: *tool_call_count,
        });
    }

    slowest.sort_by_key(|a| std::cmp::Reverse(a.duration_ms));
    slowest.truncate(SLOWEST_LIMIT);

    let duration_values: Vec<i64> = durations.iter().map(|(_, d, _)| *d).collect();

    ExecutionPerformanceProfile {
        status: state.status.clone(),
        start_time: state.start_time,
        end_time: state.end_time,
        total_duration_ms,
        total_iterations,
        total_tool_calls: state.tool_call_count,
        avg_duration_ms: avg,
        duration_distribution: distribution,
        iterations,
        bottlenecks,
        trend: analyze_trend(&duration_values),
        slowest_iterations: slowest,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::state::IterationRecord;

    fn snapshot(
        records: Vec<IterationRecord>,
        tool_calls: u32,
        start_time: i64,
        end_time: Option<i64>,
    ) -> AgentLoopStateSnapshot {
        AgentLoopStateSnapshot {
            status: ExecutionStatus::Completed,
            current_iteration: records.last().map(|r| r.iteration).unwrap_or(0),
            tool_call_count: tool_calls,
            iteration_history: records,
            start_time,
            end_time,
            error: None,
            error_records: Vec::new(),
            variable_snapshots: HashMap::new(),
        }
    }

    fn record(iteration: u32, start: i64, end: i64, tool_calls: u32) -> IterationRecord {
        IterationRecord {
            iteration,
            start_time: start,
            end_time: Some(end),
            tool_call_count: tool_calls,
            tool_calls: Vec::new(),
        }
    }

    #[test]
    fn empty_history_returns_default_profile() {
        let profile = analyze_performance(&snapshot(Vec::new(), 0, 0, None));
        assert_eq!(profile.total_iterations, 0);
        assert_eq!(profile.trend, PerformanceTrend::InsufficientData);
        assert!(profile.bottlenecks.is_empty());
        assert!(profile.iterations.is_empty());
    }

    #[test]
    fn propagates_snapshot_fields() {
        let profile = analyze_performance(&snapshot(vec![record(1, 0, 500, 2)], 2, 100, None));
        assert_eq!(profile.status, ExecutionStatus::Completed);
        assert_eq!(profile.start_time, 100);
        assert_eq!(profile.end_time, None);
        assert_eq!(profile.total_duration_ms, None);
        assert_eq!(profile.total_tool_calls, 2);
    }

    #[test]
    fn total_duration_from_snapshot_end_time() {
        let profile = analyze_performance(&snapshot(
            vec![record(1, 1000, 3000, 1)],
            1,
            1000,
            Some(9000),
        ));
        assert_eq!(profile.total_duration_ms, Some(8000));
    }

    #[test]
    fn classifies_durations() {
        let history = vec![
            record(1, 0, 500, 1),
            record(2, 1000, 4000, 2),
            record(3, 5000, 20000, 5),
        ];
        let profile = analyze_performance(&snapshot(history, 8, 0, Some(20000)));
        assert_eq!(profile.total_iterations, 3);
        assert_eq!(profile.duration_distribution.fast, 1);
        assert_eq!(profile.duration_distribution.normal, 1);
        assert_eq!(profile.duration_distribution.slow, 1);
    }

    #[test]
    fn ignores_incomplete_iterations() {
        let mut history = vec![record(1, 0, 500, 1)];
        history.push(IterationRecord {
            iteration: 2,
            start_time: 1000,
            end_time: None,
            tool_call_count: 0,
            tool_calls: Vec::new(),
        });
        let profile = analyze_performance(&snapshot(history, 1, 0, Some(2000)));
        assert_eq!(profile.total_iterations, 1);
    }

    #[test]
    fn exposes_per_iteration_detail_with_tool_calls() {
        let mut records = vec![record(1, 0, 1000, 2)];
        records[0].tool_calls = vec![
            ToolCallRecord {
                name: "read_file".to_string(),
                duration_ms: 600,
                success: true,
            },
            ToolCallRecord {
                name: "search_code".to_string(),
                duration_ms: 300,
                success: false,
            },
        ];
        let profile = analyze_performance(&snapshot(records, 2, 0, Some(1000)));
        assert_eq!(profile.iterations.len(), 1);
        assert_eq!(profile.iterations[0].tool_calls.len(), 2);
        assert_eq!(profile.iterations[0].tool_calls[1].name, "search_code");
        assert!(!profile.iterations[0].tool_calls[1].success);
    }

    #[test]
    fn detects_bottlenecks_by_multiple_of_mean() {
        let history = vec![
            record(1, 0, 1000, 1),
            record(2, 2000, 3000, 1),
            record(3, 4000, 6000, 1),
            record(4, 7000, 27000, 3),
        ];
        let profile = analyze_performance(&snapshot(history, 6, 0, Some(27000)));
        assert_eq!(profile.bottlenecks.len(), 1);
        let high = &profile.bottlenecks[0];
        assert_eq!(high.iteration, 4);
        assert_eq!(high.severity, BottleneckSeverity::High);
    }

    #[test]
    fn no_bottleneck_below_duration_floor() {
        let history = vec![
            record(1, 0, 100, 1),
            record(2, 200, 300, 1),
            record(3, 400, 500, 1),
            record(4, 600, 1200, 2),
        ];
        let profile = analyze_performance(&snapshot(history, 5, 0, Some(1200)));
        assert!(profile.bottlenecks.is_empty());
    }

    #[test]
    fn slowest_iterations_sorted_descending_and_limited() {
        let mut history = Vec::new();
        for i in 1..=15 {
            history.push(record(
                i,
                i as i64 * 1000,
                i as i64 * 1000 + i as i64 * 100,
                1,
            ));
        }
        let profile = analyze_performance(&snapshot(history, 15, 0, Some(16500)));
        assert_eq!(profile.slowest_iterations.len(), SLOWEST_LIMIT);
        assert_eq!(profile.slowest_iterations[0].iteration, 15);
    }

    #[test]
    fn trend_improving_when_second_half_faster() {
        let mut history = Vec::new();
        for i in 1..=6 {
            let duration = if i <= 3 { 2000 } else { 500 };
            history.push(record(i, i as i64 * 3000, i as i64 * 3000 + duration, 1));
        }
        let profile = analyze_performance(&snapshot(history, 6, 0, Some(21000)));
        assert_eq!(profile.trend, PerformanceTrend::Improving);
    }

    #[test]
    fn trend_degrading_when_second_half_slower() {
        let mut history = Vec::new();
        for i in 1..=6 {
            let duration = if i <= 3 { 500 } else { 2000 };
            history.push(record(i, i as i64 * 3000, i as i64 * 3000 + duration, 1));
        }
        let profile = analyze_performance(&snapshot(history, 6, 0, Some(21000)));
        assert_eq!(profile.trend, PerformanceTrend::Degrading);
    }

    #[test]
    fn trend_stable_within_20_percent() {
        let mut history = Vec::new();
        for i in 1..=6 {
            let duration = if i <= 3 { 1000 } else { 1100 };
            history.push(record(i, i as i64 * 3000, i as i64 * 3000 + duration, 1));
        }
        let profile = analyze_performance(&snapshot(history, 6, 0, Some(21300)));
        assert_eq!(profile.trend, PerformanceTrend::Stable);
    }

    #[test]
    fn trend_requires_four_iterations() {
        let mut history = Vec::new();
        for i in 1..=3 {
            history.push(record(i, i as i64 * 1000, i as i64 * 1000 + 100, 1));
        }
        let profile = analyze_performance(&snapshot(history, 3, 0, Some(3300)));
        assert_eq!(profile.trend, PerformanceTrend::InsufficientData);
    }
}
