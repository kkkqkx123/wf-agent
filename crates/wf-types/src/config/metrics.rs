use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetricCollectorConfig {
    pub buffer_size: Option<u32>,
    pub flush_interval: Option<i64>,
    pub enable_periodic_reporting: Option<bool>,
    pub reporting_interval: Option<i64>,
    pub max_age: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetricsConfig {
    pub workflow_metrics: Option<MetricCollectorConfig>,
    pub node_metrics: Option<MetricCollectorConfig>,
    pub agent_metrics: Option<MetricCollectorConfig>,
    pub event_metrics: Option<MetricCollectorConfig>,
    pub tool_metrics: Option<MetricCollectorConfig>,
    pub token_metrics: Option<MetricCollectorConfig>,
    pub template_metrics: Option<MetricCollectorConfig>,
    pub config_metrics: Option<MetricCollectorConfig>,
    pub error_metrics: Option<MetricCollectorConfig>,
    pub resource_metrics: Option<MetricCollectorConfig>,
    pub agent_loop_metrics: Option<MetricCollectorConfig>,
    pub subgraph_metrics: Option<MetricCollectorConfig>,
    pub enable_periodic_reporting: Option<bool>,
    pub reporting_interval: Option<i64>,
    pub enabled: Option<bool>,
    /// Optional HTTP address for the metrics export server, e.g.
    /// `127.0.0.1:9090`. Absent when no server should be started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_addr: Option<String>,
}
