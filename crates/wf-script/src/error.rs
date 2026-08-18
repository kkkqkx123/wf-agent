use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("{0}")]
    Internal(String),
}

pub type ScriptResult<T> = Result<T, ScriptError>;
