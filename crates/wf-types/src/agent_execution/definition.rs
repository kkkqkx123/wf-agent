use serde::{Deserialize, Serialize};

use crate::agent_execution::AgentExecutionStatus;
use crate::agent_execution::AgentRuntimeConfig;
use crate::agent_execution::IterationRecord;
use crate::checkpoint::agent::AgentStateSnapshot;
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

impl From<AgentStateSnapshot> for AgentExecution {
    fn from(snapshot: AgentStateSnapshot) -> Self {
        let status =
            serde_json::from_str::<AgentExecutionStatus>(&format!("\"{}\"", snapshot.status))
                .unwrap_or(AgentExecutionStatus::Created);

        Self {
            id: snapshot.agent_loop_id,
            definition_id: String::new(),
            status,
            current_iteration: snapshot.current_iteration,
            tool_call_count: snapshot.tool_call_count,
            iteration_history: None,
            started_at: snapshot.started_at.unwrap_or(0),
            completed_at: snapshot.completed_at,
            error: snapshot.error,
            context: None,
        }
    }
}
