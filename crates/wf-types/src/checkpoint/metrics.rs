use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointCreationMetrics {
    pub duration_ms: u64,
    pub size_bytes: u64,
    pub node_count: u32,
    pub variable_count: u32,
}
