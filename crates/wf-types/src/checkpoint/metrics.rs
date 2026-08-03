use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointCreationMetrics {
    pub duration_ms: u64,
    pub size_bytes: u64,
    pub node_count: u32,
    pub variable_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointCleanupMetrics {
    pub deleted_count: u32,
    pub freed_bytes: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointLoadMetrics {
    pub duration_ms: u64,
    pub size_bytes: u64,
    pub compressed: bool,
}

/// Chain-length metric recorded per checkpoint creation, aligned with the
/// TS `CheckpointChainLengthMetric`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointChainLengthMetric {
    pub entity_id: String,
    pub chain_length: u32,
    pub delta_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointMetricsAggregate {
    pub total_checkpoints: u64,
    pub total_size_bytes: u64,
    pub avg_creation_time_ms: f64,
    pub full_checkpoints: u64,
    pub delta_checkpoints: u64,
    pub creation_failures: u64,
    pub cleanup_count: u64,
    pub freed_bytes: u64,
    pub avg_cleanup_duration_ms: f64,
    pub load_count: u64,
    pub load_success: u64,
    pub load_failed: u64,
    pub avg_load_duration_ms: f64,
    pub chain_length_count: u64,
    pub avg_chain_length: f64,
    pub max_chain_length: u64,
    pub entities_tracked: u64,
}
