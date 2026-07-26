use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecution {
    pub id: super::super::Id,
    pub workflow_id: super::super::Id,
    pub status: super::WorkflowExecutionStatus,
    pub started_at: super::super::Timestamp,
    pub completed_at: Option<super::super::Timestamp>,
    pub error: Option<String>,
}
