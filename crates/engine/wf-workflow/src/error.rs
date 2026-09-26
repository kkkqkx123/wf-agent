use thiserror::Error;
use wf_types::workflow::error_branch::NodeErrorCategory;

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("Coordinator error: {0}")]
    CoordinatorError(String),

    /// Wall-clock (`max_execution_time`) exhaustion of a whole execution.
    /// Kept distinct from `CoordinatorError` so callers and the classifier
    /// see a timeout, not a generic coordinator failure.
    #[error("Execution timeout: {0}")]
    ExecutionTimeout(String),

    #[error("Graph error: {0}")]
    GraphError(String),

    #[error("Handler not found: {node_type}")]
    HandlerNotFound { node_type: String },

    #[error("Node execution failed: {node_id} - {reason}")]
    NodeExecutionFailed { node_id: String, reason: String },

    /// Terminal node failure carrying its routing category. Raised where the
    /// engine knows the failure kind (coordinator timeout, interruption, the
    /// emitting node's compression failure) so error-branch routing classifies
    /// by type instead of by message substring.
    #[error("Node failure [{category}] {node_id}: {detail}")]
    NodeFailure {
        node_id: String,
        category: NodeErrorCategory,
        detail: String,
    },

    #[error("Fork/Join error: {0}")]
    ForkJoinError(String),

    #[error("Subgraph error: {0}")]
    SubgraphError(String),

    #[error("Trigger error: {0}")]
    TriggerError(String),

    #[error("Variable error: {0}")]
    VariableError(String),

    #[error("Loop error: {0}")]
    LoopError(String),

    #[error("Operation error: {0}")]
    OperationError(String),

    #[error("Config error: node '{node_id}' field '{field}' is invalid: {detail}")]
    ConfigError {
        node_id: String,
        field: String,
        detail: String,
    },

    #[error("State transition error: {0}")]
    StateTransitionError(String),

    #[error("Shared error: {0}")]
    SharedError(#[from] wf_execution_shared::error::ExecutionSharedError),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type WorkflowResult<T> = Result<T, WorkflowError>;

/// Bridge into the shared handler boundary. Failures whose nature the engine
/// already knows (`NodeFailure`, a bare node execution failure, a wall-clock
/// timeout, a variable or state failure) keep their typed shared-side shape,
/// so error-branch routing never downgrades them to `BusinessFailure` and the
/// agent-side analysis can read them structurally. Everything else surfaces
/// as a `HandlerError` carrying the full message.
impl From<WorkflowError> for wf_execution_shared::error::ExecutionSharedError {
    fn from(value: WorkflowError) -> Self {
        use wf_execution_shared::error::ExecutionSharedError as Shared;
        match value {
            WorkflowError::NodeFailure {
                node_id,
                category,
                detail,
            } => Shared::NodeFailure {
                node_id,
                category,
                detail,
            },
            WorkflowError::NodeExecutionFailed { node_id, reason } => Shared::NodeFailure {
                node_id,
                category: NodeErrorCategory::BusinessFailure,
                detail: reason,
            },
            WorkflowError::ExecutionTimeout(detail) => Shared::TimeoutError(detail),
            WorkflowError::VariableError(detail) => Shared::VariableError(detail),
            WorkflowError::StateTransitionError(detail) => Shared::StateError(detail),
            other => Shared::HandlerError(other.to_string()),
        }
    }
}
