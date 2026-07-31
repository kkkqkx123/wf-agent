use std::collections::HashMap;

use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::labels;

pub const EVENT_COUNT: &str = "event.count";

/// Event statistics aggregated by event type.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct EventStats {
    pub total: u64,
    pub by_type: HashMap<String, u64>,
}

/// Domain collector for execution event metrics, fed by the event bridge.
#[derive(Clone)]
pub struct EventMetricsCollector {
    inner: BaseMetricCollector,
}

impl EventMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    pub fn record_event(
        &self,
        event_type: &str,
        execution_id: Option<&str>,
        workflow_id: Option<&str>,
    ) {
        let mut pairs = vec![("event_type", event_type)];
        if let Some(exec_id) = execution_id {
            pairs.push(("execution_id", exec_id));
        }
        if let Some(wf_id) = workflow_id {
            pairs.push(("workflow_id", wf_id));
        }
        self.inner.increment_counter(EVENT_COUNT, labels(&pairs));
    }

    /// Event counts grouped by event type.
    pub fn stats_by_type(&self) -> HashMap<String, u64> {
        let mut by_type: HashMap<String, u64> = HashMap::new();
        if let Some(agg) = self
            .inner
            .query(&crate::metric::MetricFilter {
                name: Some(EVENT_COUNT.to_string()),
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == EVENT_COUNT)
        {
            for group in agg.by_label {
                if let Some(event_type) = group.labels.get("event_type") {
                    *by_type.entry(event_type.clone()).or_insert(0) += group.value as u64;
                }
            }
        }
        by_type
    }

    pub fn total(&self) -> u64 {
        crate::collectors::counter_total(&self.inner, EVENT_COUNT) as u64
    }

    pub fn stats(&self) -> EventStats {
        EventStats {
            total: self.total(),
            by_type: self.stats_by_type(),
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

    fn collector() -> EventMetricsCollector {
        EventMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_and_groups_events() {
        let c = collector();
        c.record_event("NodeStarted", Some("exec-1"), Some("wf-1"));
        c.record_event("NodeCompleted", Some("exec-1"), Some("wf-1"));
        c.record_event("NodeStarted", Some("exec-2"), Some("wf-1"));
        let stats = c.stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.by_type.get("NodeStarted"), Some(&2));
        assert_eq!(stats.by_type.get("NodeCompleted"), Some(&1));
    }
}
