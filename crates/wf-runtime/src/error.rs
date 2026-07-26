use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Storage initialization failed: {0}")]
    Storage(#[from] wf_storage::error::StorageError),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Logger initialization failed: {0}")]
    Logger(String),

    #[error("Shutdown timed out after {0}ms")]
    ShutdownTimeout(u64),

    #[error("Signal handler error: {0}")]
    Signal(String),

    #[error("Already initialized")]
    AlreadyInitialized,

    #[error("Not initialized")]
    NotInitialized,

    #[error("Storage manager in wrong state: expected {expected}, actual {actual}")]
    InvalidState { expected: String, actual: String },
}

pub type RuntimeResult<T> = Result<T, RuntimeError>;
