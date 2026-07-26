use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionStateSnapshot {
    pub execution_id: super::super::super::Id,
    pub status: String,
    pub current_node_id: Option<String>,
    pub node_states: serde_json::Value,
    pub variable_state: super::super::CheckpointVariableState,
}
