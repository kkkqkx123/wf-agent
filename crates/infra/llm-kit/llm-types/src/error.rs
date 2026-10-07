//! Error types shared across llm-kit crates.

use thiserror::Error;

/// Result alias used by llm-kit crates.
pub type Result<T> = std::result::Result<T, Error>;

/// Unified error type for shared LLM capabilities.
#[derive(Debug, Error)]
pub enum Error {
    /// The request payload is invalid before it reaches the transport.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// The remote provider returned a non-success response.
    #[error("provider returned {status}: {message}")]
    Provider {
        /// HTTP status code reported by the provider.
        status: u16,
        /// Short error description from the provider response.
        message: String,
    },

    /// The provider response could not be decoded.
    #[error("failed to decode provider response: {0}")]
    Decode(String),

    /// The request was cancelled or timed out.
    #[error("request cancelled or timed out")]
    Cancelled,
}
