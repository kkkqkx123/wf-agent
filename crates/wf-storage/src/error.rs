#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Entity not found: {0}")]
    NotFound(String),
    #[error("Entity already exists: {0}")]
    AlreadyExists(String),
    #[error("Storage connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Write operation failed: {0}")]
    WriteFailed(String),
    #[error("Read operation failed: {0}")]
    ReadFailed(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
}

impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        StorageError::SerializationError(e.to_string())
    }
}
