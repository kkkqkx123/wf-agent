use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptExecutorConfig {
    pub executor_type: String,
    pub timeout_seconds: Option<u64>,
    pub max_memory_mb: Option<u64>,
    pub allowed_paths: Option<Vec<String>>,
}
