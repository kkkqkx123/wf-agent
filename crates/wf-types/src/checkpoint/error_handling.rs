use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointError {
    pub message: String,
    pub operation: String,
    pub checkpoint_id: Option<String>,
}
