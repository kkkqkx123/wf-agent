use std::sync::Arc;

use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::retry_metrics;
use crate::labels;
use wf_common::retry::{RetryBudgetEvent, RetryBudgetEventType};

/// Retry budget outcome summary (success/failure/success-rate).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RetryBudgetOutcomes {
    pub succeeded: u64,
    pub failed: u64,
    pub success_rate: f64,
}

/// Retry budget consumption summary.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RetryBudgetConsumptionSummary {
    pub consumed_count: u64,
    pub consumed_time_ms: u64,
    pub exhaustion_count: u64,
    pub exhaustion_by_type: std::collections::HashMap<String, u64>,
    pub remaining_count: u64,
    pub remaining_time_ms: u64,
}

/// Domain collector for retry budget metrics (consumption, exhaustion,
/// backoff, timeout errors, per-consumer active retry tracking). Listens to
/// `RetryBudgetEvent` through its `on_event` callback.
#[derive(Clone)]
pub struct RetryBudgetMetricsCollector {
    inner: BaseMetricCollector,
}

impl RetryBudgetMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record retry budget consumption.
    pub fn record_budget_consumption(
        &self,
        consumed_count: u64,
        consumed_time_ms: u64,
        labels: impl Into<std::collections::HashMap<String, String>>,
    ) {
        let label_map = labels.into();
        if consumed_count > 0 {
            self.inner.increment_counter_by(
                retry_metrics::BUDGET_CONSUMED_COUNT,
                consumed_count as f64,
                label_map.clone(),
            );
        }
        if consumed_time_ms > 0 {
            self.inner.observe_histogram(
                retry_metrics::BUDGET_CONSUMED_TIME,
                consumed_time_ms as f64,
                label_map,
            );
        }
    }

    /// Update remaining budget gauges.
    pub fn record_budget_remaining(
        &self,
        remaining_count: u32,
        remaining_time_ms: u64,
        labels: impl Into<std::collections::HashMap<String, String>>,
    ) {
        let label_map = labels.into();
        self.inner.set_gauge(
            retry_metrics::BUDGET_REMAINING_COUNT,
            remaining_count as f64,
            label_map.clone(),
        );
        self.inner.set_gauge(
            retry_metrics::BUDGET_REMAINING_TIME,
            remaining_time_ms as f64,
            label_map,
        );
    }

    /// Record budget exhaustion event with the categorical type.
    pub fn record_budget_exhausted(
        &self,
        exhaustion_type: &str,
        labels: impl Into<std::collections::HashMap<String, String>>,
    ) {
        let mut label_map = labels.into();
        label_map.insert("exhaustion_type".to_string(), exhaustion_type.to_string());
        self.inner
            .increment_counter(retry_metrics::BUDGET_EXHAUSTED, label_map);
    }

    /// Record a retry attempt (total, delay histogram, active retries).
    pub fn record_retry_attempt(&self, consumer_id: &str, delay_ms: u64, attempt_number: u32) {
        let label_map = labels(&[
            ("consumer_id", consumer_id),
            ("attempt_number", &attempt_number.to_string()),
        ]);
        self.inner
            .increment_counter(retry_metrics::ATTEMPT_TOTAL, label_map.clone());
        self.inner
            .observe_histogram(retry_metrics::DELAY_DURATION, delay_ms as f64, label_map);
        self.inner.set_gauge(
            retry_metrics::CONSUMER_ACTIVE_RETRIES,
            1.0,
            labels(&[("consumer_id", consumer_id)]),
        );
    }

    /// Record a successful retry chain outcome.
    pub fn record_retry_success(
        &self,
        consumer_id: &str,
        total_attempts: u32,
        total_delay_ms: u64,
    ) {
        let label_map = labels(&[
            ("consumer_id", consumer_id),
            ("total_attempts", &total_attempts.to_string()),
        ]);
        self.inner
            .increment_counter(retry_metrics::ATTEMPT_SUCCEEDED, label_map.clone());
        self.inner
            .increment_counter(retry_metrics::ULTIMATELY_SUCCEEDED, label_map);
        if total_delay_ms > 0 {
            self.inner.observe_histogram(
                retry_metrics::DELAY_DURATION,
                total_delay_ms as f64,
                labels(&[("consumer_id", consumer_id), ("aggregated", "true")]),
            );
        }
        self.inner.set_gauge(
            retry_metrics::CONSUMER_ACTIVE_RETRIES,
            0.0,
            labels(&[("consumer_id", consumer_id)]),
        );
    }

    /// Record a failed retry chain outcome.
    pub fn record_retry_failure(&self, consumer_id: &str, total_attempts: u32, reason: &str) {
        let label_map = labels(&[
            ("consumer_id", consumer_id),
            ("total_attempts", &total_attempts.to_string()),
            ("failure_reason", reason),
        ]);
        self.inner
            .increment_counter(retry_metrics::ATTEMPT_FAILED, label_map.clone());
        self.inner
            .increment_counter(retry_metrics::ULTIMATELY_FAILED, label_map);
        self.inner.set_gauge(
            retry_metrics::CONSUMER_ACTIVE_RETRIES,
            0.0,
            labels(&[("consumer_id", consumer_id)]),
        );
    }

    /// Record a timeout error inside a retry path.
    pub fn record_timeout_error(&self, consumer_id: &str, timeout_ms: u64, actual_ms: u64) {
        let label_map = labels(&[
            ("consumer_id", consumer_id),
            ("timeout_configured_ms", &timeout_ms.to_string()),
        ]);
        self.inner
            .increment_counter(retry_metrics::TIMEOUT_ERROR_COUNT, label_map.clone());
        self.inner
            .increment_counter(retry_metrics::TIMEOUT_ERROR_NO_RETRY, label_map);
        self.inner.observe_histogram(
            "retry.timeout.actual_ms",
            actual_ms as f64,
            labels(&[("consumer_id", consumer_id)]),
        );
    }

    /// Record backoff factor gauge for the current attempt.
    pub fn record_backoff_factor(
        &self,
        consumer_id: &str,
        base_delay_ms: u64,
        multiplier: u32,
        attempt_number: u32,
        calculated_delay_ms: u64,
    ) {
        let label_map = labels(&[
            ("consumer_id", consumer_id),
            ("base_delay_ms", &base_delay_ms.to_string()),
            ("multiplier", &multiplier.to_string()),
            ("attempt_number", &attempt_number.to_string()),
        ]);
        self.inner.set_gauge(
            retry_metrics::BACKOFF_FACTOR,
            multiplier as f64,
            label_map.clone(),
        );
        self.inner.observe_histogram(
            "retry.backoff.calculated_delay_ms",
            calculated_delay_ms as f64,
            label_map,
        );
    }

    /// Map a `RetryBudgetEvent` into metric recordings. Suitable as the
    /// `on_event` callback body for a shared `RetryBudget`.
    pub fn record_budget_event(&self, event: &RetryBudgetEvent, workflow_id: Option<&str>) {
        let wf_labels = workflow_id
            .map(|wf| labels(&[("workflow_id", wf)]))
            .unwrap_or_default();
        match event.event_type {
            RetryBudgetEventType::RetryConsumed => {
                self.inner
                    .increment_counter(retry_metrics::BUDGET_CONSUMED_COUNT, wf_labels.clone());
                if let Some(delay_ms) = event.delay_ms {
                    self.inner.observe_histogram(
                        retry_metrics::BUDGET_CONSUMED_TIME,
                        delay_ms as f64,
                        wf_labels.clone(),
                    );
                    self.inner.observe_histogram(
                        retry_metrics::DELAY_DURATION,
                        delay_ms as f64,
                        labels(&[
                            (
                                "consumer_id",
                                event.branch_id.as_deref().unwrap_or("global"),
                            ),
                            ("attempt_number", &event.retries_consumed.to_string()),
                        ]),
                    );
                }
                let remaining_count = event
                    .max_retries
                    .map(|m| m.saturating_sub(event.retries_consumed))
                    .unwrap_or(0);
                let remaining_time_ms = event
                    .time_budget_ms
                    .map(|b| b.saturating_sub(event.time_budget_consumed_ms))
                    .unwrap_or(0);
                self.inner.set_gauge(
                    retry_metrics::BUDGET_REMAINING_COUNT,
                    remaining_count as f64,
                    wf_labels.clone(),
                );
                self.inner.set_gauge(
                    retry_metrics::BUDGET_REMAINING_TIME,
                    remaining_time_ms as f64,
                    wf_labels.clone(),
                );
            }
            RetryBudgetEventType::BudgetExhausted => {
                let ex_type = if event.time_budget_ms.map(|b| b > 0).unwrap_or(false)
                    && event.time_budget_consumed_ms >= event.time_budget_ms.unwrap_or(0)
                {
                    if event
                        .max_retries
                        .map(|m| event.retries_consumed >= m)
                        .unwrap_or(false)
                    {
                        "both"
                    } else {
                        "time"
                    }
                } else {
                    "count"
                };
                self.record_budget_exhausted(ex_type, wf_labels.clone());
            }
            RetryBudgetEventType::RetryDenied => {
                let reason = event.reason.as_deref().unwrap_or("unknown");
                let ex_type = if reason.contains("count") || reason.contains("retry") {
                    "count"
                } else if reason.contains("time") || reason.contains("budget") {
                    "time"
                } else {
                    "count"
                };
                self.record_budget_exhausted(ex_type, wf_labels);
            }
            RetryBudgetEventType::BudgetReset => {
                self.inner.set_gauge(
                    retry_metrics::BUDGET_REMAINING_COUNT,
                    0.0,
                    wf_labels.clone(),
                );
                self.inner
                    .set_gauge(retry_metrics::BUDGET_REMAINING_TIME, 0.0, wf_labels);
            }
        }
    }

    /// Build an `on_event` closure suitable for wiring into
    /// `RetryBudgetConfig`. The collector is cheaply cloned inside the
    /// closure, so the original collector stays usable.
    pub fn event_handler(
        this: Arc<Self>,
        workflow_id: impl Into<String>,
    ) -> wf_common::retry::RetryBudgetEventHandler {
        let wf = workflow_id.into();
        Box::new(move |event| {
            this.clone().record_budget_event(event, Some(wf.as_str()));
        })
    }

    pub fn consumption_summary(&self) -> RetryBudgetConsumptionSummary {
        let latest = |name: &str| {
            self.inner
                .latest_snapshots(&crate::metric::MetricFilter {
                    name: Some(name.to_string()),
                    ..Default::default()
                })
                .into_iter()
                .find(|m| m.name == name)
                .map(|m| m.value)
                .unwrap_or(0.0)
        };
        let mut exhaustion_by_type = std::collections::HashMap::new();
        let result = self.inner.query(&crate::metric::MetricFilter {
            name: Some(retry_metrics::BUDGET_EXHAUSTED.to_string()),
            ..Default::default()
        });
        for m in result.metrics {
            for group in m.by_label {
                if let Some(et) = group.labels.get("exhaustion_type") {
                    *exhaustion_by_type.entry(et.clone()).or_default() += group.value as u64;
                }
            }
        }
        RetryBudgetConsumptionSummary {
            consumed_count: crate::collectors::counter_total(
                &self.inner,
                retry_metrics::BUDGET_CONSUMED_COUNT,
            ) as u64,
            consumed_time_ms: crate::collectors::counter_total(
                &self.inner,
                retry_metrics::BUDGET_CONSUMED_TIME,
            ) as u64,
            exhaustion_count: crate::collectors::counter_total(
                &self.inner,
                retry_metrics::BUDGET_EXHAUSTED,
            ) as u64,
            exhaustion_by_type,
            remaining_count: latest(retry_metrics::BUDGET_REMAINING_COUNT) as u64,
            remaining_time_ms: latest(retry_metrics::BUDGET_REMAINING_TIME) as u64,
        }
    }

    pub fn outcomes(&self) -> RetryBudgetOutcomes {
        let succeeded =
            crate::collectors::counter_total(&self.inner, retry_metrics::ULTIMATELY_SUCCEEDED)
                as u64;
        let failed =
            crate::collectors::counter_total(&self.inner, retry_metrics::ULTIMATELY_FAILED) as u64;
        let total = succeeded + failed;
        RetryBudgetOutcomes {
            succeeded,
            failed,
            success_rate: if total > 0 {
                succeeded as f64 / total as f64
            } else {
                0.0
            },
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
    use std::sync::Arc;

    fn collector() -> RetryBudgetMetricsCollector {
        RetryBudgetMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_budget_lifecycle() {
        let c = collector();
        c.record_budget_consumption(2, 300, labels(&[("workflow_id", "wf-1")]));
        c.record_budget_remaining(3, 1200, labels(&[("workflow_id", "wf-1")]));
        c.record_budget_exhausted("count", labels(&[("workflow_id", "wf-1")]));
        c.record_retry_attempt("n1", 100, 0);
        c.record_retry_success("n1", 2, 200);
        c.record_timeout_error("n1", 5000, 5001);
        c.record_backoff_factor("n1", 100, 2, 0, 100);

        let outcomes = c.outcomes();
        assert_eq!(outcomes.succeeded, 1);
        assert_eq!(outcomes.failed, 0);
        assert_eq!(c.consumption_summary().consumed_count, 2);
        assert_eq!(c.consumption_summary().exhaustion_count, 1);
    }

    #[test]
    fn record_budget_event_maps_event_types() {
        let c = collector();
        let consumed = RetryBudgetEvent {
            event_type: RetryBudgetEventType::RetryConsumed,
            retries_consumed: 2,
            max_retries: Some(5),
            time_budget_consumed_ms: 100,
            time_budget_ms: Some(500),
            branch_id: Some("b1".into()),
            reason: None,
            delay_ms: Some(50),
        };
        c.record_budget_event(&consumed, Some("wf-1"));
        let summary = c.consumption_summary();
        assert_eq!(summary.consumed_count, 1);
        assert_eq!(summary.remaining_count, 3);
        assert_eq!(summary.remaining_time_ms, 400);

        let denied = RetryBudgetEvent {
            event_type: RetryBudgetEventType::RetryDenied,
            retries_consumed: 5,
            max_retries: Some(5),
            time_budget_consumed_ms: 500,
            time_budget_ms: Some(500),
            branch_id: Some("b1".into()),
            reason: Some("count exhausted".into()),
            delay_ms: None,
        };
        c.record_budget_event(&denied, Some("wf-1"));
        let summary = c.consumption_summary();
        assert_eq!(summary.exhaustion_count, 1);
        assert_eq!(
            summary
                .exhaustion_by_type
                .get("count")
                .copied()
                .unwrap_or(0),
            1
        );
    }

    #[test]
    fn event_handler_attaches_to_budget() {
        let c = Arc::new(collector());
        let handler = RetryBudgetMetricsCollector::event_handler(c.clone(), "wf-1");
        let budget = wf_common::retry::RetryBudget::new(wf_common::retry::RetryBudgetConfig {
            max_retries: Some(2),
            time_budget_ms: Some(200),
            time_budget_mode: wf_common::retry::TimeBudgetMode::DelayOnly,
            name: "test".into(),
            on_event: Some(handler),
        });
        budget.consume_retry(10, None, 0);
        budget.consume_retry(20, None, 0);
        budget.consume_retry(30, None, 0);
        assert_eq!(c.consumption_summary().consumed_count, 2);
        assert_eq!(c.consumption_summary().exhaustion_count, 1);
    }
}
