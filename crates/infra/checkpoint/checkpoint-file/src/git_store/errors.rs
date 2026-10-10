#[derive(Debug, thiserror::Error)]
pub enum GitStoreError {
    #[error("checkpoint git store not initialized: {0}")]
    Uninitialized(String),
    #[error("ref not found: {0}")]
    RefNotFound(String),
    #[error("object not found: {0}")]
    ObjectNotFound(String),
    #[error("corrupt object {id}: {reason}")]
    Corrupt { id: String, reason: String },
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("ref transaction conflict on '{0}'")]
    RefConflict(String),
    #[error("io error: {0}")]
    Io(String),
}

impl From<std::io::Error> for GitStoreError {
    fn from(e: std::io::Error) -> Self {
        GitStoreError::Io(e.to_string())
    }
}
