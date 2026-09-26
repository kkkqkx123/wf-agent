use serde::{Deserialize, Serialize};

/// Correlation context describing which checkpoint operation failed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointErrorContext {
    pub operation: String,
    pub checkpoint_id: Option<String>,
    pub message: Option<String>,
}

/// Outcome recorded by the checkpoint error handler for one failure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointErrorHandlingResult {
    pub recovered: bool,
    pub error: Option<String>,
}
