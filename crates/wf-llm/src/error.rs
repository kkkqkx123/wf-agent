use thiserror::Error;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Stream error: {0}")]
    StreamError(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("Unsupported provider: {0:?}")]
    UnsupportedProvider(wf_types::llm::LlmProvider),

    #[error("Request timed out after {0}ms")]
    Timeout(u64),

    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    #[error("Request was cancelled")]
    Cancelled,
}

impl LlmError {
    pub fn is_retryable(&self) -> bool {
        match self {
            LlmError::HttpError(e) => e.is_timeout() || e.is_connect() || e.is_request(),
            LlmError::ProviderError(msg) => {
                // 5xx errors are retryable, 4xx are not
                msg.starts_with("HTTP 5") || msg.starts_with("HTTP 429")
            }
            LlmError::Timeout(_) | LlmError::StreamError(_) => true,
            LlmError::Cancelled
            | LlmError::SerializationError(_)
            | LlmError::ConfigError(_)
            | LlmError::ProfileNotFound(_)
            | LlmError::UnsupportedProvider(_)
            | LlmError::AuthError(_)
            | LlmError::ToolNotFound(_)
            | LlmError::InvalidResponse(_) => false,
        }
    }
}

pub type LlmResult<T> = Result<T, LlmError>;
