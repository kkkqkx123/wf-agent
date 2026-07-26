#[derive(thiserror::Error, Debug)]
pub enum CommonError {
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Internal: {0}")]
    Internal(String),

    #[error("Serialization: {0}")]
    Serialization(String),

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),


}

impl From<serde_json::Error> for CommonError {
    fn from(e: serde_json::Error) -> Self {
        CommonError::Serialization(e.to_string())
    }
}

impl From<chrono::ParseError> for CommonError {
    fn from(e: chrono::ParseError) -> Self {
        CommonError::Serialization(e.to_string())
    }
}

pub type CommonResult<T> = Result<T, CommonError>;
