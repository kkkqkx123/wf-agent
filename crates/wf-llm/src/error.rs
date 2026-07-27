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

    #[error("Timeout")]
    Timeout,

    #[error("Authentication failed")]
    AuthError,

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),
}

pub type LlmResult<T> = Result<T, LlmError>;
