use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BatchManagementOpType {
    StartNewBatch,
    RollbackToBatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchManagementOperation {
    pub batch_operation: BatchManagementOpType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_batch: Option<u32>,
}
