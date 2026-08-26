use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetricCollectorConfig {
    pub buffer_size: Option<u32>,
    pub flush_interval: Option<i64>,
    pub enable_periodic_reporting: Option<bool>,
    pub reporting_interval: Option<i64>,
}

/// Anomaly detection thresholds consumed by the report generator.
///
/// `max_error_count` triggers an error-storm anomaly, `min_success_rate`
/// (0..=1) triggers a workflow-success-degradation anomaly. Both are
/// optional; missing values fall back to the defaults.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AnomalyThresholdsConfig {
    pub max_error_count: Option<u64>,
    pub min_success_rate: Option<f64>,
}

/// Global retention window (milliseconds) driving both the in-memory
/// `cleanup_expired` and the persisted `delete_old_persisted` pruning, so
/// memory and storage share a single retention source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetricsConfig {
    pub workflow_metrics: Option<MetricCollectorConfig>,
    pub node_metrics: Option<MetricCollectorConfig>,
    pub agent_metrics: Option<MetricCollectorConfig>,
    pub event_metrics: Option<MetricCollectorConfig>,
    pub tool_metrics: Option<MetricCollectorConfig>,
    pub token_metrics: Option<MetricCollectorConfig>,
    pub config_metrics: Option<MetricCollectorConfig>,
    pub error_metrics: Option<MetricCollectorConfig>,
    pub resource_metrics: Option<MetricCollectorConfig>,
    pub agent_loop_metrics: Option<MetricCollectorConfig>,
    pub subgraph_metrics: Option<MetricCollectorConfig>,
    pub template_metrics: Option<MetricCollectorConfig>,
    pub retry_budget_metrics: Option<MetricCollectorConfig>,
    pub timeout_metrics: Option<MetricCollectorConfig>,
    pub enable_periodic_reporting: Option<bool>,
    pub reporting_interval: Option<i64>,
    pub enabled: Option<bool>,
    /// Optional HTTP address for the metrics export server, e.g.
    /// `127.0.0.1:9090`. Absent when no server should be started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_addr: Option<String>,
    /// Global retention window in milliseconds; drives both in-memory and
    /// persisted pruning (default 3600000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_ms: Option<i64>,
    /// Anomaly detection thresholds for the report generator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anomaly_thresholds: Option<AnomalyThresholdsConfig>,
}
