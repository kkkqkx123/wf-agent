use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Illegal state transition: {0}")]
    IllegalStateTransition(String),

    /// Deterministic caller/config misuse: rejected validation, resume-mode
    /// id conflicts, iteration budget above the hard cap. Retrying reproduces
    /// the same rejection.
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Execution timeout: {0}")]
    ExecutionTimeout(String),

    /// The run was cancelled (host shutdown / explicit cancellation request /
    /// dropped stream consumer), distinct from a wall-clock timeout and from
    /// a failure.
    #[error("Execution cancelled: {0}")]
    Cancelled(String),

    /// The concurrent-execution gate is momentarily full (or a resume target
    /// is still live): transient saturation that a later attempt may pass.
    #[error("Concurrency saturated: {0}")]
    ConcurrencySaturated(String),

    /// A spawn exceeded the sub-agent depth policy: a hierarchy misuse that
    /// only a configuration or call-graph change can fix.
    #[error("Sub-agent hierarchy limit reached: {0}")]
    HierarchyLimitReached(String),

    #[error("LLM error: {0}")]
    LlmError(#[from] wf_llm::error::LlmError),

    #[error("Checkpoint error: {0}")]
    CheckpointError(#[from] wf_checkpoint::error::CheckpointError),

    #[error("Shared error: {0}")]
    SharedError(#[from] wf_execution_shared::error::ExecutionSharedError),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type AgentResult<T> = Result<T, AgentError>;
