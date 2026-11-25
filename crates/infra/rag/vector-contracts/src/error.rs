//! Error type shared by the contract surface and every backend.

use serde_json::json;
use thiserror::Error;

/// Maximum characters of a remote response body kept in diagnostics.
pub const MAX_DIAGNOSTIC_BODY_CHARS: usize = 256;

/// Whether a JSON field carries credential-like content.
fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "api-key",
        "api_key",
        "token",
        "secret",
        "password",
        "authorization",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if is_secret_key(key) {
                    *child = json!("[redacted]");
                } else {
                    redact_json(child);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_json(item);
            }
        }
        _ => {}
    }
}

/// Bound and sanitize a remote body before it reaches logs or error text.
/// JSON bodies have credential-like fields replaced; non-JSON bodies are
/// kept verbatim but still bounded and stripped of control characters.
pub fn diagnostic_body(body: &str) -> String {
    let sanitized = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(mut value) => {
            redact_json(&mut value);
            value.to_string()
        }
        Err(_) => body.to_string(),
    };
    let clean: String = sanitized
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let mut truncated: String = clean.chars().take(MAX_DIAGNOSTIC_BODY_CHARS).collect();
    if clean.chars().count() > MAX_DIAGNOSTIC_BODY_CHARS {
        truncated.push_str("...");
    }
    truncated
}

/// Failure modes of vector storage operations.
#[derive(Debug, Error)]
pub enum VectorError {
    /// Caller-supplied data is invalid (empty vector, non-finite element,
    /// unknown collection name, unsupported id shape).
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// A point or query vector does not match the collection dimension.
    #[error("dimension mismatch: collection expects {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
    /// The named collection does not exist.
    #[error("collection not found: {0}")]
    CollectionMissing(String),
    /// Credentials were rejected by the remote backend.
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    /// Network or remote-service failure, including timeouts.
    #[error("transport error: {0}")]
    Transport(String),
    /// A backend response could not be decoded.
    #[error("decode error: {0}")]
    Decode(String),
    /// The backend reported an operation failure.
    #[error("backend error: {0}")]
    Backend(String),
    /// The backend does not support the requested retrieval kind.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

impl VectorError {
    /// Builds an unsupported-kind error naming the backend and kind.
    pub fn unsupported(backend: &str, kind: &str) -> Self {
        Self::Unsupported(format!("{} does not support {} vectors", backend, kind))
    }
}

/// Fallible result of vector storage operations.
pub type Result<T> = std::result::Result<T, VectorError>;
