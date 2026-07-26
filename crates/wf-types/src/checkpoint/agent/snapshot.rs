use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopStateSnapshot {
    pub agent_loop_id: super::super::super::Id,
    pub status: String,
    pub current_iteration: u32,
    pub conversation_snapshot: Option<serde_json::Value>,
    pub tool_call_history: Vec<serde_json::Value>,
}
