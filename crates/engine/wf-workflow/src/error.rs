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

    /// Main-loop pause signal turned into a typed control-flow result.
    /// Pausing is not an engine malfunction, so it must never travel as
    /// `CoordinatorError`; routing and transports read it as an
    /// interruption, never as a business failure.
    #[error("Execution paused: {0}")]
    ExecutionPaused(String),

    #[error("Graph error: {0}")]
    GraphError(String),

    #[error("Handler not found: {node_type}")]
    HandlerNotFound { node_type: String },

    #[error("Node execution failed: {node_id} - {reason}")]
    NodeExecutionFailed { node_id: String, reason: String },

    /// Terminal node failure carrying its routing category. Raised where the
    /// engine knows the failure kind (coordinator timeout, interruption, the
    /// emitting node's compression failure) so error-branch routing classifies
    /// by type instead of by message substring. `source` names the failure
    /// origin (handler, hook veto, approval) for error-branch routing; it
    /// shares the business category with handler failures, so the source
    /// travels as its own dimension.
    #[error("Node failure [{category}] {node_id}: {detail}")]
    NodeFailure {
        node_id: String,
        category: NodeErrorCategory,
        detail: String,
        failure_source: wf_types::workflow::error_branch::NodeFailureSource,
    },

    #[error("Fork/Join error: {0}")]
    ForkJoinError(String),

    #[error("Subgraph error: {0}")]
    SubgraphError(String),

    #[error("Trigger error: {0}")]
    TriggerError(String),

    #[error("Sub-workflow hierarchy limit reached: {0}")]
    HierarchyLimitReached(String),

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

/// Bridge into the shared handler boundary. Typed failures keep their
/// shared-side shape so routing never downgrades them. Engine diagnostics
/// without a node context become a categorized `NodeFailure` with a
/// placeholder id and a source prefix; validation-shaped diagnostics become
/// `VariableError`. `HandlerError` is reserved for genuinely untyped handler
/// internals. A nested shared error is unwrapped instead of being stringified
/// twice.
impl From<WorkflowError> for wf_execution_shared::error::ExecutionSharedError {
    fn from(value: WorkflowError) -> Self {
        use wf_execution_shared::error::ExecutionSharedError as Shared;
        match value {
            WorkflowError::NodeFailure {
                node_id,
                category,
                detail,
                failure_source,
            } => Shared::NodeFailure {
                node_id,
                category,
                detail,
                failure_source,
            },
            WorkflowError::NodeExecutionFailed { node_id, reason } => Shared::NodeFailure {
                node_id,
                category: NodeErrorCategory::BusinessFailure,
                detail: reason,
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            },
            WorkflowError::ExecutionTimeout(detail) => Shared::TimeoutError(detail),
            WorkflowError::ExecutionPaused(detail) => Shared::InterruptionError {
                kind: wf_execution_shared::error::InterruptionKind::Pause,
                detail,
            },
            WorkflowError::VariableError(detail) => Shared::VariableError(detail),
            WorkflowError::StateTransitionError(detail) => Shared::StateError(detail),
            WorkflowError::SharedError(inner) => inner,
            WorkflowError::GraphError(detail) => Shared::VariableError(detail),
            WorkflowError::ConfigError {
                node_id,
                field,
                detail,
            } => Shared::VariableError(format!(
                "node '{node_id}' field '{field}' is invalid: {detail}"
            )),
            WorkflowError::HandlerNotFound { node_type } => {
                Shared::VariableError(format!("Handler not found: {node_type}"))
            }
            WorkflowError::CoordinatorError(detail) => Shared::NodeFailure {
                node_id: "unknown".to_string(),
                category: NodeErrorCategory::BusinessFailure,
                detail: format!("coordinator: {detail}"),
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            },
            WorkflowError::ForkJoinError(detail) => Shared::NodeFailure {
                node_id: "unknown".to_string(),
                category: NodeErrorCategory::BusinessFailure,
                detail: format!("fork_join: {detail}"),
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            },
            WorkflowError::SubgraphError(detail) => Shared::NodeFailure {
                node_id: "unknown".to_string(),
                category: NodeErrorCategory::BusinessFailure,
                detail: format!("subgraph: {detail}"),
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            },
            WorkflowError::TriggerError(detail) => Shared::NodeFailure {
                node_id: "unknown".to_string(),
                category: NodeErrorCategory::BusinessFailure,
                detail: format!("trigger: {detail}"),
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            },
            WorkflowError::HierarchyLimitReached(detail) => Shared::NodeFailure {
                node_id: "unknown".to_string(),
                category: NodeErrorCategory::BusinessFailure,
                detail: format!("hierarchy_limit: {detail}"),
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            },
            WorkflowError::LoopError(detail) => Shared::NodeFailure {
                node_id: "unknown".to_string(),
                category: NodeErrorCategory::BusinessFailure,
                detail: format!("loop: {detail}"),
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            },
            WorkflowError::OperationError(detail) => Shared::NodeFailure {
                node_id: "unknown".to_string(),
                category: NodeErrorCategory::BusinessFailure,
                detail: format!("operation: {detail}"),
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            },
            WorkflowError::Internal(detail) => Shared::NodeFailure {
                node_id: "unknown".to_string(),
                category: NodeErrorCategory::BusinessFailure,
                detail: format!("internal: {detail}"),
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            },
        }
    }
}
