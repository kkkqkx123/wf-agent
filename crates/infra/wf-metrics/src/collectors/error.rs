use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::error_metrics;
use crate::labels;

/// Usage statistics aggregated from error records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ErrorStats {
    pub total: u64,
    pub recovered: u64,
    pub recovery_rate: f64,
    pub affected_executions: u64,
}

/// Domain collector for error occurrence and recovery metrics.
#[derive(Clone)]
pub struct ErrorMetricsCollector {
    inner: BaseMetricCollector,
}

impl ErrorMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    pub fn record_error(&self, error_type: &str, source: &str, execution_id: Option<&str>) {
        let mut pairs = vec![("error_type", error_type), ("source", source)];
        if let Some(exec_id) = execution_id {
            pairs.push(("execution_id", exec_id));
        }
        self.inner
            .increment_counter(error_metrics::OCCURRENCE_COUNT, labels(&pairs));
    }

    pub fn record_recovery(&self, source: &str, success: bool) {
        self.inner.increment_counter(
            error_metrics::RECOVERY_RATE,
            labels(&[
                ("source", source),
                ("success", if success { "true" } else { "false" }),
            ]),
        );
    }

    pub fn record_affected_execution(&self, execution_id: &str) {
        self.inner.increment_counter(
            error_metrics::AFFECTED_EXECUTIONS,
            labels(&[("execution_id", execution_id)]),
        );
    }

    pub fn stats(&self) -> ErrorStats {
        let total = crate::collectors::counter_total(&self.inner, error_metrics::OCCURRENCE_COUNT);
        let recovered = crate::collectors::counter_total(&self.inner, error_metrics::RECOVERY_RATE);
        ErrorStats {
            total: total as u64,
            recovered: recovered as u64,
            recovery_rate: if total > 0.0 { recovered / total } else { 0.0 },
            affected_executions: crate::collectors::counter_total(
                &self.inner,
                error_metrics::AFFECTED_EXECUTIONS,
            ) as u64,
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

    fn collector() -> ErrorMetricsCollector {
        ErrorMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_errors_and_recovery() {
        let c = collector();
        c.record_error("llm", "agent", Some("exec-1"));
        c.record_error("tool", "agent", Some("exec-1"));
        c.record_recovery("agent", true);
        c.record_affected_execution("exec-1");
        let stats = c.stats();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.recovered, 1);
        assert_eq!(stats.recovery_rate, 0.5);
        assert_eq!(stats.affected_executions, 1);
    }
}
