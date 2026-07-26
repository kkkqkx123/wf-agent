use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopStorageConfig {
    pub storage_type: String,
    pub connection_string: Option<String>,
}
