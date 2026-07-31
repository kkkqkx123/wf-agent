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

    pub fn record_execution_start(&self, execution_id: &str) {
        self.inner.increment_counter(
            agent_loop_metrics::EXECUTION_COUNT,
            labels(&[("execution_id", execution_id)]),
        );
        self.inner.set_gauge(
            agent_loop_metrics::ACTIVE_COUNT,
            1.0,
            labels(&[("execution_id", execution_id)]),
        );
    }

    pub fn record_execution_complete(&self, execution_id: &str, success: bool, duration_ms: f64) {
        let labels = labels(&[
            ("execution_id", execution_id),
            ("success", if success { "true" } else { "false" }),
        ]);
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
        self.inner.set_gauge(
            agent_loop_metrics::ACTIVE_COUNT,
            0.0,
            crate::labels(&[("execution_id", execution_id)]),
        );
    }

    pub fn record_iteration(&self, execution_id: &str, duration_ms: f64) {
        self.inner.increment_counter(
            agent_loop_metrics::ITERATION_COUNT,
            labels(&[("execution_id", execution_id)]),
        );
        self.inner.observe_histogram(
            agent_loop_metrics::ITERATION_DURATION,
            duration_ms,
            labels(&[("execution_id", execution_id)]),
        );
    }

    pub fn record_max_iterations_reached(&self, execution_id: &str) {
        self.inner.increment_counter(
            agent_loop_metrics::MAX_ITERATIONS_REACHED,
            labels(&[("execution_id", execution_id)]),
        );
    }

    pub fn record_tool_calls(&self, execution_id: &str, count: u64) {
        self.inner.increment_counter_by(
            agent_loop_metrics::TOOL_CALLS_TOTAL,
            count as f64,
            labels(&[("execution_id", execution_id)]),
        );
    }

    pub fn record_pause(&self, execution_id: &str) {
        self.inner.increment_counter(
            agent_loop_metrics::PAUSE_COUNT,
            labels(&[("execution_id", execution_id)]),
        );
    }

    pub fn record_resume(&self, execution_id: &str) {
        self.inner.increment_counter(
            agent_loop_metrics::RESUME_COUNT,
            labels(&[("execution_id", execution_id)]),
        );
    }

    pub fn record_error(&self, execution_id: &str, error_type: &str) {
        self.inner.increment_counter(
            agent_loop_metrics::ERROR_COUNT,
            labels(&[("execution_id", execution_id), ("error_type", error_type)]),
        );
    }

    pub fn record_protocol_locked(&self, execution_id: &str, format: &str) {
        self.inner.increment_counter(
            protocol_metrics::LOCKED_COUNT,
            labels(&[("execution_id", execution_id), ("format", format)]),
        );
    }

    pub fn record_protocol_violation(&self, execution_id: &str) {
        self.inner.increment_counter(
            protocol_metrics::VIOLATION_COUNT,
            labels(&[("execution_id", execution_id)]),
        );
    }

    pub fn record_protocol_conversion(&self, execution_id: &str) {
        self.inner.increment_counter(
            protocol_metrics::CONVERSION_COUNT,
            labels(&[("execution_id", execution_id)]),
        );
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
        c.record_execution_start("exec-1");
        c.record_iteration("exec-1", 500.0);
        c.record_execution_complete("exec-1", true, 1500.0);
        let stats = c.usage_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.iterations, 1);
        assert_eq!(stats.avg_duration_ms, 1500.0);
    }

    #[test]
    fn records_protocol_and_pause_metrics() {
        let c = collector();
        c.record_pause("exec-1");
        c.record_resume("exec-1");
        c.record_protocol_locked("exec-1", "json");
        c.record_protocol_violation("exec-1");
        c.record_error("exec-1", "llm");
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
        c.record_max_iterations_reached("exec-1");
        let stats = c.usage_stats();
        assert_eq!(stats.max_iterations_reached, 1);
    }
}
