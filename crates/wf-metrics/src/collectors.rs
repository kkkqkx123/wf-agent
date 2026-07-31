pub mod agent;
pub mod agent_loop;
pub mod error;
pub mod event;
pub mod node;
pub mod token;
pub mod tool;
pub mod workflow;

pub use agent::AgentMetricsCollector;
pub use agent_loop::AgentLoopMetricsCollector;
pub use error::ErrorMetricsCollector;
pub use event::EventMetricsCollector;
pub use node::NodeMetricsCollector;
pub use token::TokenMetricsCollector;
pub use tool::ToolMetricsCollector;
pub use workflow::WorkflowMetricsCollector;

use crate::collector::BaseMetricCollector;
use crate::metric::MetricFilter;

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
    collector
        .query(&MetricFilter {
            name: Some(name.to_string()),
            metric_type: Some(crate::metric::MetricType::Counter),
            ..Default::default()
        })
        .metrics
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.value)
        .unwrap_or(0.0)
}
