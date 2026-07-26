use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageOperationConfig {
    pub enable_deduplication: Option<bool>,
    pub enable_compression: Option<bool>,
    pub max_context_length: Option<u32>,
}
