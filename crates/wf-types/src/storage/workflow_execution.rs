use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionStorageConfig {
    pub storage_type: String,
    pub retention_days: Option<u32>,
}
