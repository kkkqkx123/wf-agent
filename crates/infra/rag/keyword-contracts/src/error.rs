//! Failures of keyword index backends.

use thiserror::Error;

/// Errors produced by the keyword storage contract.
#[derive(Debug, Error)]
pub enum KeywordError {
    /// Caller-supplied input is invalid.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Backend operation failed; the message carries the backend error.
    #[error("{0}")]
    Backend(String),
    /// Persisted index format differs from the resolved configuration.
    #[error("keyword index format mismatch: index holds '{existing}' but the configuration resolves to '{current}'; rebuild the index before use")]
    FormatMismatch {
        /// Stamp found in the existing index.
        existing: String,
        /// Stamp resolved from the current configuration.
        current: String,
    },
}

impl KeywordError {
    /// Builds a backend failure with a readable message.
    pub fn backend(message: impl Into<String>) -> Self {
        Self::Backend(message.into())
    }

    /// Builds an invalid-input failure with a readable message.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }
}

/// Fallible result of one keyword backend operation.
pub type Result<T> = std::result::Result<T, KeywordError>;
