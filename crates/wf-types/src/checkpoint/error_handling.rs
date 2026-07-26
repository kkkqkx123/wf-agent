use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointErrorStrategy {
    Silent,
    Warn,
    Strict,
    Callback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointErrorContext {
    pub operation: String,
    pub checkpoint_id: Option<String>,
    pub message: Option<String>,
    pub attempt: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointError {
    pub message: String,
    pub operation: String,
    pub checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointErrorHandlingResult {
    pub recovered: bool,
    pub retry_count: u32,
    pub error: Option<String>,
}
