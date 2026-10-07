//! Error type for the basic chat client.

use thiserror::Error;

/// Chat-specific error.
#[derive(Debug, Error)]
pub enum ChatError {
    /// The request payload is invalid before it reaches the transport.
    #[error("invalid chat request: {0}")]
    InvalidRequest(String),

    /// The remote provider returned a non-success response.
    #[error("chat provider returned {status}: {message}")]
    Provider {
        /// HTTP status code reported by the provider.
        status: u16,
        /// Short error description from the provider response.
        message: String,
    },

    /// The provider response could not be decoded.
    #[error("failed to decode chat response: {0}")]
    Decode(String),

    /// The HTTP client itself failed (network, TLS).
    #[error("chat transport failure: {0}")]
    Transport(String),

    /// The request timed out.
    #[error("chat request timed out")]
    Timeout,
}

/// Result alias for chat operations.
pub type Result<T> = std::result::Result<T, ChatError>;

impl ChatError {
    /// Whether the failed call is worth retrying (rate limits, 5xx, network,
    /// timeouts). Client errors other than 429 never retry.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transport(_) | Self::Timeout | Self::Decode(_) => true,
            Self::Provider { status, .. } => *status == 429 || (500..=599).contains(status),
            Self::InvalidRequest(_) => false,
        }
    }
}

impl From<reqwest::Error> for ChatError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            return ChatError::Timeout;
        }
        if let Some(status) = err.status() {
            ChatError::Provider {
                status: status.as_u16(),
                message: err.to_string(),
            }
        } else {
            ChatError::Transport(err.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_covers_transient_failures_only() {
        assert!(ChatError::Timeout.is_retryable());
        assert!(ChatError::Transport("down".into()).is_retryable());
        assert!(ChatError::Provider {
            status: 429,
            message: "slow down".into(),
        }
        .is_retryable());
        assert!(ChatError::Provider {
            status: 503,
            message: "unavailable".into(),
        }
        .is_retryable());
        assert!(!ChatError::Provider {
            status: 400,
            message: "bad request".into(),
        }
        .is_retryable());
        assert!(!ChatError::InvalidRequest("empty".into()).is_retryable());
    }
}
