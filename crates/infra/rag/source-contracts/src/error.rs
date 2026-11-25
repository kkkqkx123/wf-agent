//! Errors shared by source readers and converters.

use thiserror::Error;

/// Result type of the source contracts.
pub type Result<T> = std::result::Result<T, SourceError>;

/// Failures produced while reading or normalizing a source.
#[derive(Debug, Error)]
pub enum SourceError {
    #[error("invalid source descriptor: {0}")]
    InvalidDescriptor(String),
    #[error("unsupported source kind '{0}'")]
    UnsupportedKind(String),
    #[error("unknown converter '{0}'")]
    UnknownConverter(String),
    #[error("source read failed: {0}")]
    Read(String),
    #[error("source conversion failed: {0}")]
    Conversion(String),
    #[error("source pagination failed: {0}")]
    Pagination(String),
}
