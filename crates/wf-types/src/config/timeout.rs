use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeoutConfig {
    pub default_node_timeout_seconds: Option<u64>,
    pub default_workflow_timeout_seconds: Option<u64>,
    pub max_timeout_seconds: Option<u64>,
}
