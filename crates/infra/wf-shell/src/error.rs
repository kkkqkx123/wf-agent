use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShellError {
    #[error("Shell session not found: {0}")]
    NotFound(String),

    #[error("Shell validation failed: {0}")]
    ValidationFailed(String),

    #[error("Shell internal error: {0}")]
    Internal(String),

    #[error("Shell execution error: {0}")]
    ExecutionError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type ShellResult<T> = Result<T, ShellError>;
