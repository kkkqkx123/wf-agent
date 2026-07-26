use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopStateSnapshot {
    pub agent_loop_id: super::super::super::Id,
    pub status: String,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_snapshot: Option<Vec<super::super::super::message::Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_history: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_snapshots: Option<HashMap<String, VariableSnapshot>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub started_at: Option<super::super::super::Timestamp>,
    pub completed_at: Option<super::super::super::Timestamp>,
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
