use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttemptCompletionEvent {
    pub base: super::BaseEvent,
    pub content: String,
    pub data: Option<HashMap<String, serde_json::Value>>,
    pub variables: Option<HashMap<String, serde_json::Value>>,
    pub node_id: Option<String>,
}
