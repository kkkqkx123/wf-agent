//! Error type shared by the contract surface and every backend.

use thiserror::Error;

/// Failure modes of vector storage operations.
#[derive(Debug, Error)]
pub enum VectorError {
    /// Caller-supplied data is invalid (empty vector, non-finite element,
    /// unknown collection name, unsupported id shape).
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// A point or query vector does not match the collection dimension.
    #[error("dimension mismatch: collection expects {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
    /// The named collection does not exist.
    #[error("collection not found: {0}")]
    CollectionMissing(String),
    /// Credentials were rejected by the remote backend.
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    /// Network or remote-service failure, including timeouts.
    #[error("transport error: {0}")]
    Transport(String),
    /// A backend response could not be decoded.
    #[error("decode error: {0}")]
    Decode(String),
    /// The backend reported an operation failure.
    #[error("backend error: {0}")]
    Backend(String),
}

/// Fallible result of vector storage operations.
pub type Result<T> = std::result::Result<T, VectorError>;
