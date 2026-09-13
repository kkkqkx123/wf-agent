use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

/// Severity label for a todo item. Named `importance`, not `priority`:
/// it neither orders execution (ordering convention) nor arbitrates a
/// winner (trigger priority) — it only grades importance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TodoImportance {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TodoItem {
    pub id: super::Id,
    pub content: String,
    pub status: TodoStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<TodoImportance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::Metadata>,
}
