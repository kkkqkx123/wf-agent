use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmClientConfig {
    pub provider_id: String,
    pub profile_id: String,
    pub timeout_seconds: Option<u64>,
    pub max_retries: Option<u32>,
}
