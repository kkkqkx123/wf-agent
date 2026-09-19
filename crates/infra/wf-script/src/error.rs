use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("{0}")]
    Internal(String),
    #[error("review required: {0}")]
    ReviewRequired(String),
    #[error("invalid definition: {0}")]
    InvalidDefinition(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("unresolved template: {0}")]
    UnresolvedTemplate(String),
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error("payload rejected: {0}")]
    Payload(String),
}

pub type ScriptResult<T> = Result<T, ScriptError>;
