use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowConfig {
    pub max_executions: Option<u32>,
    pub timeout_seconds: Option<u64>,
    pub enable_checkpoint: Option<bool>,
    pub failure_policy: Option<String>,
}
