use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Entity error: {0}")]
    EntityError(String),

    #[error("State error: {0}")]
    StateError(String),

    #[error("Coordinator error: {0}")]
    CoordinatorError(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Hook error: {0}")]
    HookError(String),

    #[error("Tool error: {0}")]
    ToolError(#[from] wf_tools::error::ToolError),

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
