use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatelessToolConfig {
    pub parameters: Option<serde_json::Value>,
    pub timeout_seconds: Option<u64>,
    pub retry_on_failure: Option<bool>,
}
