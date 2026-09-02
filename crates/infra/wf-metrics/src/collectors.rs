pub mod agent;
pub mod agent_loop;
pub mod config;
pub mod error;
pub mod event;
pub mod node;
pub mod resource;
pub mod retry_budget;
pub mod subgraph;
pub mod template;
pub mod timeout;
pub mod token;
pub mod tool;
pub mod workflow;

pub use agent::{AgentMetricsCollector, AgentUsageStats};
pub use agent_loop::AgentLoopMetricsCollector;
pub use config::ConfigMetricsCollector;
pub use error::{ErrorMetricsCollector, ErrorStats};
pub use event::{EventMetricsCollector, EventStats};
pub use node::{NodeExecutionRecord, NodeMetricsCollector, NodeUsageStats};
pub use resource::{ResourceMetricsCollector, ResourceSample};
pub use retry_budget::{
    RetryBudgetConsumptionSummary, RetryBudgetMetricsCollector, RetryBudgetOutcomes,
};
pub use subgraph::SubgraphMetricsCollector;
pub use template::{TemplateMetricsCollector, TemplateUsageStats};
pub use timeout::{TimeoutMetricsCollector, TimeoutStats};
pub use token::TokenMetricsCollector;
pub use tool::ToolMetricsCollector;
pub use workflow::{WorkflowMetricsCollector, WorkflowUsageStats};

use crate::collector::BaseMetricCollector;
use crate::metric::{MetricFilter, MetricType};
use std::collections::HashMap;

/// Latest recorded snapshot for a metric name, used by domain stats helpers.
fn latest(collector: &BaseMetricCollector, name: &str) -> Option<crate::metric::Metric> {
    collector
        .latest_snapshots(&MetricFilter {
            name: Some(name.to_string()),
            ..Default::default()
        })
        .into_iter()
        .find(|m| m.name == name)
}

/// Sum of all counter observations recorded under `name`.
fn counter_total(collector: &BaseMetricCollector, name: &str) -> f64 {
    counter_total_labeled(collector, name, &HashMap::new())
}

/// Sum of counter observations recorded under `name` matching `labels`.
pub(crate) fn counter_total_labeled(
    collector: &BaseMetricCollector,
    name: &str,
    labels: &HashMap<String, String>,
) -> f64 {
    collector
        .query(&MetricFilter {
            name: Some(name.to_string()),
            metric_type: Some(MetricType::Counter),
            labels: if labels.is_empty() {
                None
            } else {
                Some(labels.clone())
            },
            ..Default::default()
        })
        .metrics
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.value)
        .unwrap_or(0.0)
}

/// Latest snapshot for a metric name matching `labels`.
pub(crate) fn latest_labeled(
    collector: &BaseMetricCollector,
    name: &str,
    labels: &HashMap<String, String>,
) -> Option<crate::metric::Metric> {
    collector
        .latest_snapshots(&MetricFilter {
            name: Some(name.to_string()),
            labels: if labels.is_empty() {
                None
            } else {
                Some(labels.clone())
            },
            ..Default::default()
        })
        .into_iter()
        .find(|m| m.name == name)
}

/// Counter total merged with persisted storage (history-aware).
///
/// Sums in-memory counter value and persisted points filtered by `labels`.
/// Used when `server` feature is absent and CLI queries local storage.
pub(crate) async fn counter_total_with_history(
    collector: &BaseMetricCollector,
    name: &str,
    labels: &HashMap<String, String>,
) -> f64 {
    let mem = counter_total_labeled(collector, name, labels);
    let persisted = match collector.query_sink(name, 0, wf_common::time::now()).await {
        Some(Ok(points)) => points
            .iter()
            .filter(|p| labels.iter().all(|(k, v)| p.labels.get(k) == Some(v)))
            .map(|p| p.value)
            .sum::<f64>(),
        _ => 0.0,
    };
    mem + persisted
}

/// Latest snapshot merged with persisted storage.
///
/// Prefers in-memory latest; falls back to the most recent persisted
/// histogram snapshot (rebuilt via `rebuild_persisted`) when memory is empty.
pub(crate) async fn latest_with_history(
    collector: &BaseMetricCollector,
    name: &str,
    labels: &HashMap<String, String>,
) -> Option<crate::metric::Metric> {
    if let Some(m) = latest_labeled(collector, name, labels) {
        return Some(m);
    }
    let points = match collector.query_sink(name, 0, wf_common::time::now()).await {
        Some(Ok(p)) => p,
        _ => return None,
    };
    let mut filtered: Vec<crate::sink::MetricPoint> = points
        .into_iter()
        .filter(|p| labels.iter().all(|(k, v)| p.labels.get(k) == Some(v)))
        .collect();
    if filtered.is_empty() {
        return None;
    }
    filtered.sort_by_key(|p| p.timestamp);
    let latest_point = filtered.into_iter().max_by_key(|p| p.timestamp)?;
    if let Some(m) = BaseMetricCollector::rebuild_persisted(latest_point.clone()) {
        Some(m)
    } else {
        Some(crate::metric::Metric {
            name: latest_point.name,
            metric_type: latest_point.metric_type,
            value: latest_point.value,
            timestamp: latest_point.timestamp,
            labels: latest_point.labels,
            source: latest_point.source,
            buckets: latest_point.buckets,
            percentiles: Vec::new(),
            sum: latest_point.sum,
            count: latest_point.count,
        })
    }
}
