use serde::{Deserialize, Serialize};

use crate::agent_execution::AgentExecutionStatus;
use crate::agent_execution::AgentRuntimeConfig;
use crate::agent_execution::IterationRecord;
use crate::Id;
use crate::Timestamp;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentExecution {
    pub id: Id,
    pub definition_id: Id,
    pub status: AgentExecutionStatus,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration_history: Option<Vec<IterationRecord>>,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<AgentRuntimeConfig>,
}
