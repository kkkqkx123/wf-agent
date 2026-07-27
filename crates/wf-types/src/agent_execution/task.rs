use serde::{Deserialize, Serialize};

use crate::Id;
use crate::Timestamp;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub id: Id,
    pub task_type: String,
    pub status: String,
    pub payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
