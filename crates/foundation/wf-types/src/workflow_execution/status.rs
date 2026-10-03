use serde::{Deserialize, Serialize};

pub use crate::execution::ExecutionStatus as WorkflowExecutionStatus;
pub type ExecutionStatus = crate::execution::ExecutionStatus;

/// Provenance tag recording how an execution was spawned. This is an
/// observability label for history and analysis; scheduling, cancellation
/// and checkpoint scope follow the execution hierarchy instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionType {
    Main,
    ForkJoin,
    TriggeredSubworkflow,
    Subgraph,
}

impl WorkflowExecutionType {
    /// Whether the execution is the root of its hierarchy.
    pub fn is_main(&self) -> bool {
        matches!(self, Self::Main)
    }

    /// Whether the execution was spawned as a child of another execution.
    pub fn is_child(&self) -> bool {
        !self.is_main()
    }
}
