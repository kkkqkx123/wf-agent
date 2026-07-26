use serde::{Deserialize, Serialize};

use super::batch_management_operation::BatchManagementOperation;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageOperationType {
    Append,
    Insert,
    Replace,
    Truncate,
    Clear,
    Filter,
    Rollback,
    BatchManagement,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "operation", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageOperationConfig {
    Append(AppendMessageOperation),
    Insert(InsertMessageOperation),
    Replace(ReplaceMessageOperation),
    Truncate(TruncateMessageOperation),
    Clear(ClearMessageOperation),
    Filter(FilterMessageOperation),
    Rollback(RollbackMessageOperation),
    BatchManagement(BatchManagementOperation),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppendMessageOperation {
    pub messages: Vec<super::Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InsertMessageOperation {
    pub messages: Vec<super::Message>,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplaceMessageOperation {
    pub index: u32,
    pub message: super::Message,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TruncateMessageOperation {
    pub keep_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_end: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClearMessageOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilterMessageOperation {
    pub role: Option<super::MessageRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RollbackMessageOperation {
    pub target_batch: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageOperationResult {
    pub messages: Vec<super::Message>,
    pub affected_batch_index: Option<u32>,
    pub stats: MessageOperationStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageOperationStats {
    pub added: u32,
    pub removed: u32,
    pub modified: u32,
    pub total_after: u32,
}
