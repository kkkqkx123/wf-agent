use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::agent_metrics;
use crate::labels;

/// Usage statistics aggregated from agent execution records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AgentUsageStats {
    pub total: u64,
    pub success: u64,
    pub failure: u64,
    pub success_rate: f64,
    pub total_iterations: u64,
    pub total_tool_calls: u64,
    pub avg_duration_ms: f64,
    pub p95_duration_ms: f64,
}

/// Domain collector for agent execution metrics, broken down by profile.
#[derive(Clone)]
pub struct AgentMetricsCollector {
    inner: BaseMetricCollector,
}

impl AgentMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    pub fn record_execution_start(&self, profile_id: &str, execution_id: &str) {
        self.inner.increment_counter(
            agent_metrics::EXECUTION_COUNT,
            labels(&[("profile_id", profile_id), ("execution_id", execution_id)]),
        );
    }

    pub fn record_execution_complete(
        &self,
        profile_id: &str,
        execution_id: &str,
        success: bool,
        duration_ms: f64,
    ) {
        let labels = labels(&[
            ("profile_id", profile_id),
            ("execution_id", execution_id),
            ("success", if success { "true" } else { "false" }),
        ]);
        self.inner.increment_counter(
            if success {
                agent_metrics::SUCCESS_COUNT
            } else {
                agent_metrics::FAILURE_COUNT
            },
            labels.clone(),
        );
        self.inner
            .observe_histogram(agent_metrics::EXECUTION_DURATION, duration_ms, labels);
    }

    pub fn record_iteration(&self, profile_id: &str) {
        self.inner
            .increment_counter(agent_metrics::ITERATION_COUNT, labels(&[("profile_id", profile_id)]));
    }

    pub fn record_tool_call(&self, profile_id: &str) {
        self.inner
            .increment_counter(agent_metrics::TOOL_CALL_COUNT, labels(&[("profile_id", profile_id)]));
    }

    pub fn usage_stats(&self) -> AgentUsageStats {
        self.usage_stats_filtered(&std::collections::HashMap::new())
    }

    /// Usage statistics scoped to a single agent profile.
    pub fn usage_stats_for(&self, profile_id: &str) -> AgentUsageStats {
        self.usage_stats_filtered(&crate::labels(&[("profile_id", profile_id)]))
    }

    fn usage_stats_filtered(
        &self,
        filter: &std::collections::HashMap<String, String>,
    ) -> AgentUsageStats {
        let total = crate::collectors::counter_total_labeled(&self.inner, agent_metrics::EXECUTION_COUNT, filter);
        let success = crate::collectors::counter_total_labeled(&self.inner, agent_metrics::SUCCESS_COUNT, filter);
        let failure = crate::collectors::counter_total_labeled(&self.inner, agent_metrics::FAILURE_COUNT, filter);
        let duration = crate::collectors::latest_labeled(&self.inner, agent_metrics::EXECUTION_DURATION, filter);
        let total_iterations =
            crate::collectors::counter_total_labeled(&self.inner, agent_metrics::ITERATION_COUNT, filter);
        let total_tool_calls =
            crate::collectors::counter_total_labeled(&self.inner, agent_metrics::TOOL_CALL_COUNT, filter);

        AgentUsageStats {
            total: total as u64,
            success: success as u64,
            failure: failure as u64,
            success_rate: if total > 0.0 { success / total } else { 0.0 },
            total_iterations: total_iterations as u64,
            total_tool_calls: total_tool_calls as u64,
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

    fn collector() -> AgentMetricsCollector {
        AgentMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_agent_execution() {
        let c = collector();
        c.record_execution_start("default", "exec-1");
        c.record_execution_complete("default", "exec-1", true, 1000.0);
        c.record_iteration("default");
        c.record_tool_call("default");
        let stats = c.usage_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.total_iterations, 1);
        assert_eq!(stats.total_tool_calls, 1);
        assert_eq!(stats.avg_duration_ms, 1000.0);
    }

    #[test]
    fn records_agent_failure() {
        let c = collector();
        c.record_execution_start("profile-a", "exec-1");
        c.record_execution_complete("profile-a", "exec-1", false, 200.0);
        let stats = c.usage_stats();
        assert_eq!(stats.failure, 1);
        assert_eq!(stats.success_rate, 0.0);
    }
}
