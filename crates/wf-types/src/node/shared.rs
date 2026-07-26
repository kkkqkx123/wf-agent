use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeIdentity {
    pub id: super::super::Id,
    pub name: Option<String>,
    pub node_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeExecutionConfig {
    pub max_retries: Option<u32>,
    pub timeout_seconds: Option<u64>,
    pub failure_policy: Option<String>,
}
