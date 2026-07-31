//! Lightweight agent execution performance profiling.
//!
//! Pure functions over `AgentLoopState::iteration_history`; no persistence.
//! The TS `PerformanceMetricsAPI` full version (per-iteration token/cost
//! breakdown, cross-execution comparison) is deliberately not implemented:
//! its data sources do not exist in Rust and the TS results were unreliable
//! (hardcoded counts, model-level aggregation).

use serde::{Deserialize, Serialize};

use crate::state::IterationRecord;

const FAST_MS: i64 = 1_000;
const NORMAL_MS: i64 = 5_000;
const BOTTLENECK_MEDIUM_FACTOR: f64 = 1.5;
const BOTTLENECK_HIGH_FACTOR: f64 = 2.5;
const IMPROVING_FACTOR: f64 = 0.8;
const DEGRADING_FACTOR: f64 = 1.2;
const MIN_TREND_ITERATIONS: usize = 4;
const SLOWEST_LIMIT: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurationClass {
    Fast,
    Normal,
    Slow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceTrend {
    Improving,
    Degrading,
    Stable,
    #[default]
    InsufficientData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BottleneckSeverity {
    Medium,
    High,
}

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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPerformanceProfile {
    pub total_iterations: u32,
    pub avg_duration_ms: f64,
    pub duration_distribution: DurationDistribution,
    pub bottlenecks: Vec<Bottleneck>,
    pub trend: PerformanceTrend,
    pub slowest_iterations: Vec<SlowIteration>,
}

/// Profile an agent execution from its iteration history.
///
/// Only completed iterations (`end_time` set) contribute durations. Trend
/// needs at least 4 iterations; fewer yield `InsufficientData`.
pub fn analyze_performance(history: &[IterationRecord]) -> ExecutionPerformanceProfile {
    let durations: Vec<(u32, i64, u32)> = history
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
        return ExecutionPerformanceProfile::default();
    }

    let sum: i64 = durations.iter().map(|(_, duration, _)| *duration).sum();
    let avg = sum as f64 / total_iterations as f64;

    let mut distribution = DurationDistribution::default();
    let mut bottlenecks = Vec::new();
    let mut slowest: Vec<SlowIteration> = Vec::with_capacity(durations.len());

    for (iteration, duration, tool_calls) in &durations {
        match *duration {
            d if d < FAST_MS => distribution.fast += 1,
            d if d < NORMAL_MS => distribution.normal += 1,
            _ => distribution.slow += 1,
        }

        let duration_f = *duration as f64;
        let severity = if duration_f > avg * BOTTLENECK_HIGH_FACTOR {
            Some(BottleneckSeverity::High)
        } else if duration_f > avg * BOTTLENECK_MEDIUM_FACTOR {
            Some(BottleneckSeverity::Medium)
        } else {
            None
        };
        if let Some(severity) = severity {
            bottlenecks.push(Bottleneck {
                iteration: *iteration,
                duration_ms: *duration,
                severity,
            });
        }

        slowest.push(SlowIteration {
            iteration: *iteration,
            duration_ms: *duration,
            tool_call_count: *tool_calls,
        });
    }

    slowest.sort_by_key(|a| std::cmp::Reverse(a.duration_ms));
    slowest.truncate(SLOWEST_LIMIT);

    ExecutionPerformanceProfile {
        total_iterations,
        avg_duration_ms: avg,
        duration_distribution: distribution,
        bottlenecks,
        trend: trend(durations.iter().map(|(_, d, _)| *d).collect()),
        slowest_iterations: slowest,
    }
}

/// Compare the mean of the second half against the first half of the
/// iteration durations.
fn trend(durations: Vec<i64>) -> PerformanceTrend {
    if durations.len() < MIN_TREND_ITERATIONS {
        return PerformanceTrend::InsufficientData;
    }
    let mid = durations.len() / 2;
    let (first, second) = durations.split_at(mid);
    let mean = |slice: &[i64]| slice.iter().sum::<i64>() as f64 / slice.len() as f64;
    let first_mean = mean(first);
    let second_mean = mean(second);

    if first_mean <= 0.0 {
        return PerformanceTrend::Stable;
    }
    let ratio = second_mean / first_mean;
    if ratio < IMPROVING_FACTOR {
        PerformanceTrend::Improving
    } else if ratio > DEGRADING_FACTOR {
        PerformanceTrend::Degrading
    } else {
        PerformanceTrend::Stable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(iteration: u32, start: i64, end: i64, tool_calls: u32) -> IterationRecord {
        IterationRecord {
            iteration,
            start_time: start,
            end_time: Some(end),
            tool_call_count: tool_calls,
        }
    }

    #[test]
    fn empty_history_returns_default_profile() {
        let profile = analyze_performance(&[]);
        assert_eq!(profile.total_iterations, 0);
        assert_eq!(profile.trend, PerformanceTrend::InsufficientData);
        assert!(profile.bottlenecks.is_empty());
    }

    #[test]
    fn classifies_durations() {
        let history = vec![
            record(1, 0, 500, 1),
            record(2, 1000, 4000, 2),
            record(3, 5000, 20000, 5),
        ];
        let profile = analyze_performance(&history);
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
        });
        let profile = analyze_performance(&history);
        assert_eq!(profile.total_iterations, 1);
    }

    #[test]
    fn detects_bottlenecks_by_multiple_of_mean() {
        let history = vec![
            record(1, 0, 100, 1),
            record(2, 1000, 1100, 1),
            record(3, 2000, 2200, 1),
            record(4, 3000, 5000, 3),
        ];
        let profile = analyze_performance(&history);
        assert!(!profile.bottlenecks.is_empty());
        let high = profile
            .bottlenecks
            .iter()
            .find(|b| b.iteration == 4)
            .expect("slowest iteration should be a bottleneck");
        assert_eq!(high.severity, BottleneckSeverity::High);
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
        let profile = analyze_performance(&history);
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
        let profile = analyze_performance(&history);
        assert_eq!(profile.trend, PerformanceTrend::Improving);
    }

    #[test]
    fn trend_degrading_when_second_half_slower() {
        let mut history = Vec::new();
        for i in 1..=6 {
            let duration = if i <= 3 { 500 } else { 2000 };
            history.push(record(i, i as i64 * 3000, i as i64 * 3000 + duration, 1));
        }
        let profile = analyze_performance(&history);
        assert_eq!(profile.trend, PerformanceTrend::Degrading);
    }

    #[test]
    fn trend_stable_within_20_percent() {
        let mut history = Vec::new();
        for i in 1..=6 {
            let duration = if i <= 3 { 1000 } else { 1100 };
            history.push(record(i, i as i64 * 3000, i as i64 * 3000 + duration, 1));
        }
        let profile = analyze_performance(&history);
        assert_eq!(profile.trend, PerformanceTrend::Stable);
    }

    #[test]
    fn trend_requires_four_iterations() {
        let mut history = Vec::new();
        for i in 1..=3 {
            history.push(record(i, i as i64 * 1000, i as i64 * 1000 + 100, 1));
        }
        let profile = analyze_performance(&history);
        assert_eq!(profile.trend, PerformanceTrend::InsufficientData);
    }
}
