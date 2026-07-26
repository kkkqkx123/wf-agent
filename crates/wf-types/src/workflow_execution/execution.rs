use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionOptions {
    pub initial_variables: Option<serde_json::Value>,
    pub max_execution_time: Option<u64>,
    pub enable_checkpoint: Option<bool>,
    pub checkpoint_interval: Option<u32>,
}
