use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::{agent_loop_metrics, protocol_metrics};
use crate::labels;

/// Usage statistics aggregated from agent loop records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AgentLoopUsageStats {
    pub total: u64,
    pub success: u64,
    pub failure: u64,
    pub success_rate: f64,
    pub iterations: u64,
    pub max_iterations_reached: u64,
    pub pause_count: u64,
    pub resume_count: u64,
    pub protocol_locked: u64,
    pub protocol_violations: u64,
    pub errors: u64,
    pub avg_duration_ms: f64,
    pub p95_duration_ms: f64,
}

/// Domain collector for the agent loop execution loop and protocol metrics.
#[derive(Clone)]
pub struct AgentLoopMetricsCollector {
    inner: BaseMetricCollector,
}

impl AgentLoopMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record the start of an agent loop execution. Labels are bounded
    /// dimensions only; the active-execution gauge is sampled by the runtime
    /// `ResourceSampler` (M2/L1).
    pub fn record_execution_start(&self) {
        self.inner
            .increment_counter(agent_loop_metrics::EXECUTION_COUNT, labels(&[]));
    }

    pub fn record_execution_complete(&self, success: bool, duration_ms: f64) {
        let labels = labels(&[("success", if success { "true" } else { "false" })]);
        self.inner.increment_counter(
            if success {
                agent_loop_metrics::SUCCESS_RATE
            } else {
                agent_loop_metrics::ERROR_COUNT
            },
            labels.clone(),
        );
        self.inner
            .observe_histogram(agent_loop_metrics::EXECUTION_DURATION, duration_ms, labels);
    }

    pub fn record_iteration(&self, duration_ms: f64) {
        self.inner
            .increment_counter(agent_loop_metrics::ITERATION_COUNT, labels(&[]));
        self.inner.observe_histogram(
            agent_loop_metrics::ITERATION_DURATION,
            duration_ms,
            labels(&[]),
        );
    }

    pub fn record_max_iterations_reached(&self) {
        self.inner
            .increment_counter(agent_loop_metrics::MAX_ITERATIONS_REACHED, labels(&[]));
    }

    pub fn record_tool_calls(&self, count: u64) {
        self.inner.increment_counter_by(
            agent_loop_metrics::TOOL_CALLS_TOTAL,
            count as f64,
            labels(&[]),
        );
    }

    pub fn record_pause(&self) {
        self.inner
            .increment_counter(agent_loop_metrics::PAUSE_COUNT, labels(&[]));
    }

    pub fn record_resume(&self) {
        self.inner
            .increment_counter(agent_loop_metrics::RESUME_COUNT, labels(&[]));
    }

    pub fn record_error(&self, error_type: &str) {
        self.inner.increment_counter(
            agent_loop_metrics::ERROR_COUNT,
            labels(&[("error_type", error_type)]),
        );
    }

    pub fn record_protocol_locked(&self, format: &str) {
        self.inner.increment_counter(
            protocol_metrics::LOCKED_COUNT,
            labels(&[("format", format)]),
        );
    }

    pub fn record_protocol_violation(&self) {
        self.inner
            .increment_counter(protocol_metrics::VIOLATION_COUNT, labels(&[]));
    }

    pub fn record_protocol_conversion(&self) {
        self.inner
            .increment_counter(protocol_metrics::CONVERSION_COUNT, labels(&[]));
    }

    pub fn usage_stats(&self) -> AgentLoopUsageStats {
        let total =
            crate::collectors::counter_total(&self.inner, agent_loop_metrics::EXECUTION_COUNT);
        let success =
            crate::collectors::counter_total(&self.inner, agent_loop_metrics::SUCCESS_RATE);
        let duration =
            crate::collectors::latest(&self.inner, agent_loop_metrics::EXECUTION_DURATION);

        AgentLoopUsageStats {
            total: total as u64,
            success: success as u64,
            failure: crate::collectors::counter_total(&self.inner, agent_loop_metrics::ERROR_COUNT)
                as u64,
            success_rate: if total > 0.0 { success / total } else { 0.0 },
            iterations: crate::collectors::counter_total(
                &self.inner,
                agent_loop_metrics::ITERATION_COUNT,
            ) as u64,
            max_iterations_reached: crate::collectors::counter_total(
                &self.inner,
                agent_loop_metrics::MAX_ITERATIONS_REACHED,
            ) as u64,
            pause_count: crate::collectors::counter_total(
                &self.inner,
                agent_loop_metrics::PAUSE_COUNT,
            ) as u64,
            resume_count: crate::collectors::counter_total(
                &self.inner,
                agent_loop_metrics::RESUME_COUNT,
            ) as u64,
            protocol_locked: crate::collectors::counter_total(
                &self.inner,
                protocol_metrics::LOCKED_COUNT,
            ) as u64,
            protocol_violations: crate::collectors::counter_total(
                &self.inner,
                protocol_metrics::VIOLATION_COUNT,
            ) as u64,
            errors: crate::collectors::counter_total(&self.inner, agent_loop_metrics::ERROR_COUNT)
                as u64,
            avg_duration_ms: duration
                .as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
            p95_duration_ms: duration
                .as_ref()
                .and_then(|d| {
                    d.percentiles
                        .iter()
                        .find(|q| (q.percentile - 0.95).abs() < f64::EPSILON)
                })
                .map(|q| q.value)
                .unwrap_or(0.0),
        }
    }

    pub fn to_prometheus(&self) -> String {
        crate::formatter::format_collector_prometheus(&self.inner)
    }

    pub fn to_json(&self) -> serde_json::Value {
        crate::formatter::format_collector_json(&self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collector() -> AgentLoopMetricsCollector {
        AgentLoopMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_loop_lifecycle() {
        let c = collector();
        c.record_execution_start();
        c.record_iteration(500.0);
        c.record_execution_complete(true, 1500.0);
        let stats = c.usage_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.iterations, 1);
        assert_eq!(stats.avg_duration_ms, 1500.0);
    }

    #[test]
    fn records_protocol_and_pause_metrics() {
        let c = collector();
        c.record_pause();
        c.record_resume();
        c.record_protocol_locked("json");
        c.record_protocol_violation();
        c.record_error("llm");
        let stats = c.usage_stats();
        assert_eq!(stats.pause_count, 1);
        assert_eq!(stats.resume_count, 1);
        assert_eq!(stats.protocol_locked, 1);
        assert_eq!(stats.protocol_violations, 1);
        assert_eq!(stats.errors, 1);
    }

    #[test]
    fn records_max_iterations_reached() {
        let c = collector();
        c.record_max_iterations_reached();
        let stats = c.usage_stats();
        assert_eq!(stats.max_iterations_reached, 1);
    }

    #[test]
    fn loop_labels_are_bounded() {
        // No `execution_id` label may reach the collector (M2/L1); the
        // active-execution gauge is sampled by the runtime `ResourceSampler`.
        let c = collector();
        c.record_execution_start();
        c.record_iteration(10.0);
        c.record_tool_calls(3);
        c.record_execution_complete(true, 20.0);
        let active = c
            .inner
            .query(&crate::metric::MetricFilter {
                name: Some(agent_loop_metrics::ACTIVE_COUNT.to_string()),
                ..Default::default()
            })
            .metrics;
        assert!(
            active.is_empty(),
            "ACTIVE_COUNT is no longer recorded per execution"
        );
        let filtered = c.inner.query(&crate::metric::MetricFilter::default());
        let mut all_labels = std::collections::HashSet::new();
        for m in &filtered.metrics {
            for g in &m.by_label {
                for key in g.labels.keys() {
                    all_labels.insert(key.clone());
                }
            }
        }
        assert!(
            !all_labels.contains("execution_id"),
            "recorded label set must not contain execution_id: {all_labels:?}"
        );
    }
}
