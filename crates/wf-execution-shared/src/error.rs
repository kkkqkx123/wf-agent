use thiserror::Error;

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

    #[error("Pool error: {0}")]
    PoolError(String),

    #[error("Condition error: {0}")]
    ConditionError(String),

    #[error("LLM error: {0}")]
    LlmError(#[from] wf_llm::error::LlmError),

    #[error("Tool error: {0}")]
    ToolError(#[from] wf_tools::error::ToolError),

    #[error("Core error: {0}")]
    CoreError(#[from] wf_core::error::CoreError),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type ExecutionSharedResult<T> = Result<T, ExecutionSharedError>;
