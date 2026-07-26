use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::message::Message;
use crate::Id;
use crate::Timestamp;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentStateSnapshot {
    pub agent_loop_id: Id,
    pub status: String,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_snapshot: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_history: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_snapshots: Option<HashMap<String, VariableSnapshot>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableSnapshot {
    pub value: serde_json::Value,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub updated: i64,
    pub source: String,
}
