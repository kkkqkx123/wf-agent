use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopExecution {
    pub id: super::super::Id,
    pub definition_id: super::super::Id,
    pub status: super::AgentLoopStatus,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration_history: Option<Vec<super::IterationRecord>>,
    pub started_at: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<super::super::Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<super::AgentLoopRuntimeConfig>,
}
