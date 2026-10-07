//! Error type for the rerank crate.

use thiserror::Error;

/// Rerank-specific error.
#[derive(Debug, Error)]
pub enum RerankError {
    /// The request payload is invalid before it reaches the transport.
    #[error("invalid rerank request: {0}")]
    InvalidRequest(String),

    /// The remote provider returned a non-success response.
    #[error("rerank provider returned {status}: {message}")]
    Provider {
        /// HTTP status code reported by the provider.
        status: u16,
        /// Short error description from the provider response.
        message: String,
    },

    /// The provider response could not be decoded.
    #[error("failed to decode rerank response: {0}")]
    Decode(String),

    /// The HTTP client itself failed (network, TLS, timeout).
    #[error("rerank transport failure: {0}")]
    Transport(String),

    /// The rerank call exceeded its deadline.
    #[error("rerank request timed out")]
    Timeout,
}

/// Result alias for rerank operations.
pub type Result<T> = std::result::Result<T, RerankError>;

impl From<reqwest::Error> for RerankError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            return RerankError::Timeout;
        }
        if let Some(status) = err.status() {
            RerankError::Provider {
                status: status.as_u16(),
                message: err.to_string(),
            }
        } else {
            RerankError::Transport(err.to_string())
        }
    }
}

impl From<tokio::time::error::Elapsed> for RerankError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        RerankError::Timeout
    }
}
