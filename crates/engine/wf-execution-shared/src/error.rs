use thiserror::Error;
use wf_common::gate::GateError;
use wf_types::workflow::error_branch::NodeErrorCategory;

/// Kind of control-flow interruption carried by [`ExecutionSharedError::InterruptionError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptionKind {
    Stop,
    Pause,
    Abort,
}

impl std::fmt::Display for InterruptionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Stop => "stop",
            Self::Pause => "pause",
            Self::Abort => "abort",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Error)]
pub enum ExecutionSharedError {
    #[error("Interruption ({kind}): {detail}")]
    InterruptionError {
        kind: InterruptionKind,
        detail: String,
    },

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("State error: {0}")]
    StateError(String),

    #[error("Hook error: {0}")]
    HookError(String),

    #[error("Gate error: {0}")]
    GateError(String),

    #[error("Condition error: {0}")]
    ConditionError(String),

    #[error("Variable error: {0}")]
    VariableError(String),

    #[error("Tool error: {0}")]
    ToolError(#[from] wf_tools::error::ToolError),

    /// Error produced by a node handler of an execution engine. Engines map
    /// their internal error types into this variant at the `NodeHandler`
    /// trait boundary (see `wf_workflow::error`).
    #[error("Handler error: {0}")]
    HandlerError(String),

    /// Terminal node failure that carries its routing category across the
    /// handler boundary, so error-branch routing reads the category by type
    /// rather than by matching the message text. `source` names the failure
    /// origin for error-branch routing.
    #[error("Node failure [{category}] {node_id}: {detail}")]
    NodeFailure {
        node_id: String,
        category: NodeErrorCategory,
        detail: String,
        failure_source: wf_types::workflow::error_branch::NodeFailureSource,
    },

    #[error("Core error: {0}")]
    CoreError(#[from] wf_core::error::CoreError),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type ExecutionSharedResult<T> = Result<T, ExecutionSharedError>;

impl From<GateError> for ExecutionSharedError {
    fn from(e: GateError) -> Self {
        match e {
            GateError::Closed(msg) => ExecutionSharedError::GateError(msg),
        }
    }
}
