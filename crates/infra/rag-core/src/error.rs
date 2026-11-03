//! RAG failure modes. Provider errors cross the boundary as messages so
//! this crate never depends on provider error types.

use thiserror::Error;

/// Failures of RAG ingest and query pipelines.
#[derive(Debug, Error)]
pub enum RagError {
    /// Caller-supplied input is invalid (empty document, bad config).
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Vector storage operation failed.
    #[error("storage error: {0}")]
    Store(#[from] vector_contracts::VectorError),
    /// Embedding call failed; the message carries the provider error.
    #[error("embedding error: {0}")]
    Embedding(String),
    /// Rerank call failed; the message carries the provider error.
    #[error("rerank error: {0}")]
    Rerank(String),
    /// Model call failed; the message carries the provider error.
    #[error("model error: {0}")]
    Llm(String),
    /// Document conversion failed.
    #[error("conversion error: {0}")]
    Conversion(String),
    /// A stored point lacks the payload RAG ingest always writes.
    #[error("point '{0}' misses RAG payload")]
    MissingPayload(String),
}

/// Fallible result of RAG operations.
pub type Result<T> = std::result::Result<T, RagError>;
