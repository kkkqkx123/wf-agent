use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserInteractionNodeConfig {
    pub prompt: String,
    pub required: Option<bool>,
    pub timeout_seconds: Option<u64>,
    pub response_variable: Option<String>,
}
