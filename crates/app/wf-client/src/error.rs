//! Error type for the generated client wrapper.

/// Failures produced by [`crate::WfClient`].
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("http error: {0}")]
    Http(String),
    #[error("remote error {code}: {message}")]
    Remote { code: String, message: String },
    #[error("unauthorized: missing or invalid API key")]
    Unauthorized,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid response: {0}")]
    Invalid(String),
    #[error("stream error: {0}")]
    Stream(String),
    #[error("not configured: {0}")]
    NotConfigured(String),
}

/// Convenience result alias.
pub type ClientResult<T> = Result<T, ClientError>;

/// Map a `progenitor_client::Error` into [`ClientError`] without exposing the
/// generated error generics to callers.
pub fn map_progenitor<E>(err: progenitor_client::Error<E>) -> ClientError
where
    E: std::fmt::Debug,
{
    use progenitor_client::Error as P;
    match err {
        P::InvalidRequest(msg) => ClientError::Http(format!("invalid request: {msg}")),
        P::CommunicationError(err) => ClientError::Http(err.to_string()),
        P::InvalidUpgrade(err) => ClientError::Stream(err.to_string()),
        P::ErrorResponse(_) => ClientError::Http("error response".to_string()),
        P::ResponseBodyError(err) => ClientError::Invalid(err.to_string()),
        P::InvalidResponsePayload(_, err) => ClientError::Invalid(err.to_string()),
        P::UnexpectedResponse(resp) => {
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED {
                ClientError::Unauthorized
            } else if status == reqwest::StatusCode::NOT_FOUND {
                ClientError::NotFound(status.to_string())
            } else {
                ClientError::Http(format!("unexpected HTTP {status}"))
            }
        }
        P::Custom(msg) => ClientError::Http(msg),
    }
}
