use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionStatus {
    Created,
    Running,
    Paused,
    Stopped,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionType {
    Main,
    ForkJoin,
    TriggeredSubworkflow,
    Subgraph,
}

pub type ExecutionStatus = WorkflowExecutionStatus;
