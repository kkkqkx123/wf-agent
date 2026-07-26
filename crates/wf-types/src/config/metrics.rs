use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub export_interval_seconds: Option<u64>,
    pub include_node_metrics: Option<bool>,
    pub include_llm_metrics: Option<bool>,
    pub include_tool_metrics: Option<bool>,
}
