use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    pub name: String,
    pub url: String,
    pub api_key: Option<String>,
    pub timeout_seconds: Option<u64>,
}
