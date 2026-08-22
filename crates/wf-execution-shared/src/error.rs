use thiserror::Error;
use wf_common::gate::GateError;

#[derive(Debug, Error)]
pub enum ExecutionSharedError {
    #[error("Interruption error: {0}")]
    InterruptionError(String),

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
