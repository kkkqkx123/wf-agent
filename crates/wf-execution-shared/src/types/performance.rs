//! Shared performance analysis primitives for agent loop and workflow
//! executions (duration classification, trend, bottleneck detection).

use serde::{Deserialize, Serialize};

pub const FAST_MS: i64 = 1_000;
pub const NORMAL_MS: i64 = 5_000;
pub const BOTTLENECK_MEDIUM_FACTOR: f64 = 1.5;
pub const BOTTLENECK_HIGH_FACTOR: f64 = 2.5;
pub const IMPROVING_FACTOR: f64 = 0.8;
pub const DEGRADING_FACTOR: f64 = 1.2;
pub const MIN_TREND_ITERATIONS: usize = 4;

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

pub fn classify_duration(duration_ms: i64) -> DurationClass {
    match duration_ms {
        d if d < FAST_MS => DurationClass::Fast,
        d if d < NORMAL_MS => DurationClass::Normal,
        _ => DurationClass::Slow,
    }
}

/// Classify an iteration/node duration as a bottleneck relative to the mean.
///
/// Durations below `NORMAL_MS` are never flagged, avoiding noise from short
/// executions where a few samples skew the mean.
pub fn classify_bottleneck(duration_ms: i64, avg_duration_ms: f64) -> Option<BottleneckSeverity> {
    if duration_ms < NORMAL_MS {
        return None;
    }
    let duration = duration_ms as f64;
    if duration > avg_duration_ms * BOTTLENECK_HIGH_FACTOR {
        Some(BottleneckSeverity::High)
    } else if duration > avg_duration_ms * BOTTLENECK_MEDIUM_FACTOR {
        Some(BottleneckSeverity::Medium)
    } else {
        None
    }
}

/// Compare the mean of the second half against the first half of the
/// durations. Needs at least `MIN_TREND_ITERATIONS` samples; fewer yield
/// `InsufficientData`.
pub fn analyze_trend(durations: &[i64]) -> PerformanceTrend {
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

    #[test]
    fn classifies_duration_tiers() {
        assert_eq!(classify_duration(500), DurationClass::Fast);
        assert_eq!(classify_duration(3000), DurationClass::Normal);
        assert_eq!(classify_duration(8000), DurationClass::Slow);
    }

    #[test]
    fn bottleneck_respects_absolute_floor() {
        assert_eq!(classify_bottleneck(4000, 1000.0), None);
        assert_eq!(classify_bottleneck(8000, 1000.0), Some(BottleneckSeverity::High));
        assert_eq!(
            classify_bottleneck(6000, 3000.0),
            Some(BottleneckSeverity::Medium)
        );
    }

    #[test]
    fn trend_requires_minimum_samples() {
        assert_eq!(analyze_trend(&[1, 2, 3]), PerformanceTrend::InsufficientData);
    }

    #[test]
    fn trend_detects_improving() {
        let durations = vec![2000, 2000, 2000, 500, 500, 500];
        assert_eq!(analyze_trend(&durations), PerformanceTrend::Improving);
    }

    #[test]
    fn trend_detects_degrading() {
        let durations = vec![500, 500, 500, 2000, 2000, 2000];
        assert_eq!(analyze_trend(&durations), PerformanceTrend::Degrading);
    }
}
