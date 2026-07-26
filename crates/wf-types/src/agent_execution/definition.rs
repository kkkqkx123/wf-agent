use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopExecution {
    pub id: super::super::Id,
    pub definition_id: super::super::Id,
    pub status: super::AgentLoopStatus,
    pub current_iteration: u32,
    pub context: Option<super::AgentLoopRuntimeConfig>,
}
