use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchManagementOperation {
    pub operation_type: String,
    pub message_ids: Vec<super::super::Id>,
    pub metadata: Option<serde_json::Value>,
}
