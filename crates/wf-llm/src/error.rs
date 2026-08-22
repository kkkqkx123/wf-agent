use thiserror::Error;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Provider error: {0}")]
    ProviderError(String),

    /// The provider rejected the request because the actual payload exceeds
    /// its context window (anthropic `context_length_exceeded`, openai
    /// `context_length_exceeded` / "maximum context length", ...). This is
    /// the safety-net trigger for forced compression events when local
    /// estimation undercounted.
    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Stream error: {0}")]
    StreamError(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("Unsupported provider: {0:?}")]
    UnsupportedProvider(wf_types::llm::LlmProvider),

    #[error("Formatter not registered for provider: {0}")]
    FormatterNotFound(String),

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
            | LlmError::FormatterNotFound(_)
            | LlmError::AuthError(_)
            | LlmError::ToolNotFound(_)
            | LlmError::InvalidResponse(_)
            | LlmError::ContextLengthExceeded(_) => false,
        }
    }

    /// Classify an error as a context-length-exceeded rejection of the
    /// *actual* request payload. Matches the provider error codes across the
    /// supported providers (anthropic `context_length_exceeded`, openai
    /// `context_length_exceeded` / "maximum context length").
    pub fn is_context_length_exceeded(&self) -> bool {
        fn matches(msg: &str) -> bool {
            msg.contains("context_length_exceeded")
                || msg.contains("maximum context length")
                || msg.contains("context length exceeded")
                || msg.contains("Context length exceeded")
        }
        match self {
            LlmError::ContextLengthExceeded(_) => true,
            LlmError::ProviderError(msg) | LlmError::StreamError(msg) => matches(msg),
            _ => false,
        }
    }
}

pub type LlmResult<T> = Result<T, LlmError>;

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::LlmProvider;

    async fn http_err() -> reqwest::Error {
        // A real reqwest error from an impossible request (port 1, no HTTP).
        let client = reqwest::Client::new();
        let request = client
            .request(reqwest::Method::GET, "http://127.0.0.1:1")
            .build()
            .expect("request must build");
        client
            .execute(request)
            .await
            .expect_err("connection must be refused")
    }

    #[test]
    fn provider_5xx_is_retryable() {
        assert!(LlmError::ProviderError("HTTP 500 internal".to_string()).is_retryable());
        assert!(LlmError::ProviderError("HTTP 503 unavailable".to_string()).is_retryable());
        assert!(LlmError::ProviderError("HTTP 429 too many".to_string()).is_retryable());
    }

    #[test]
    fn provider_4xx_is_not_retryable() {
        assert!(!LlmError::ProviderError("HTTP 400 bad request".to_string()).is_retryable());
        assert!(!LlmError::ProviderError("HTTP 401 unauthorized".to_string()).is_retryable());
    }

    #[test]
    fn provider_error_must_carry_http_prefix() {
        assert!(!LlmError::ProviderError("500 internal".to_string()).is_retryable());
    }

    #[test]
    fn timeout_and_stream_errors_are_retryable() {
        assert!(LlmError::Timeout(5000).is_retryable());
        assert!(LlmError::StreamError("stream reset".to_string()).is_retryable());
    }

    #[test]
    fn non_retryable_variants() {
        for err in [
            LlmError::SerializationError(
                serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
            ),
            LlmError::ConfigError("bad".to_string()),
            LlmError::ProfileNotFound("p".to_string()),
            LlmError::UnsupportedProvider(LlmProvider::Custom("x".to_string())),
            LlmError::FormatterNotFound("x".to_string()),
            LlmError::AuthError("denied".to_string()),
            LlmError::ToolNotFound("t".to_string()),
            LlmError::InvalidResponse("bad body".to_string()),
            LlmError::ContextLengthExceeded("too long".to_string()),
            LlmError::Cancelled,
        ] {
            assert!(!err.is_retryable(), "must not retry: {err:?}");
        }
    }

    #[tokio::test]
    async fn http_connect_errors_are_retryable() {
        let err = LlmError::HttpError(http_err().await);
        assert!(err.is_retryable(), "connect errors must be retryable");
    }

    #[test]
    fn explicit_context_length_exceeded_is_detected() {
        assert!(LlmError::ContextLengthExceeded("nope".to_string()).is_context_length_exceeded());
    }

    #[test]
    fn provider_messages_classify_as_context_length() {
        for msg in [
            "Error: context_length_exceeded",
            "This model's maximum context length is 200000",
            "context length exceeded while processing the request",
            "Context length exceeded: the request exceeds the limit",
        ] {
            let err = LlmError::ProviderError(msg.to_string());
            assert!(err.is_context_length_exceeded(), "must classify: {msg}");
        }
        let err = LlmError::StreamError("stream failed with context_length_exceeded".to_string());
        assert!(err.is_context_length_exceeded());
    }

    #[test]
    fn unrelated_messages_are_not_context_length() {
        let err = LlmError::ProviderError("HTTP 429 rate limit".to_string());
        assert!(!err.is_context_length_exceeded());
        assert!(!LlmError::Timeout(100).is_context_length_exceeded());
        assert!(!LlmError::AuthError("denied".to_string()).is_context_length_exceeded());
    }
}
