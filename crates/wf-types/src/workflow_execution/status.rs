use serde::{Deserialize, Serialize};

pub use crate::execution::ExecutionStatus as WorkflowExecutionStatus;
pub type ExecutionStatus = crate::execution::ExecutionStatus;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionType {
    Main,
    ForkJoin,
    TriggeredSubworkflow,
    Subgraph,
}
