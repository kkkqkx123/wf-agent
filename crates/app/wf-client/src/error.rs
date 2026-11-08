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

/// Shared status-to-error mapping used by both the hand-written envelope
/// path and the progenitor path so the two never disagree.
///
/// Stable server codes survive verbatim: `TICKET_INVALID` (403),
/// `SESSION_BUSY` (409), `SESSION_NOT_FOUND` / `LOCATION_NOT_FOUND` (404)
/// stay in `Remote { code, .. }` so callers can branch on them. Only
/// generic 404s without a stable code collapse to `NotFound`.
pub fn from_status_body(status: u16, body: &serde_json::Value) -> ClientError {
    if status == 401 {
        return ClientError::Unauthorized;
    }
    let body_code = body
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let body_message = body
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str());
    if status == 404 {
        match body_code {
            "SESSION_NOT_FOUND" | "LOCATION_NOT_FOUND" | "TICKET_INVALID" | "SESSION_BUSY" => {
                return ClientError::Remote {
                    code: body_code.to_string(),
                    message: body_message.unwrap_or("not found").to_string(),
                };
            }
            _ => {
                return ClientError::NotFound(body_message.unwrap_or("not found").to_string());
            }
        }
    }
    let (default_code, default_message) = match status {
        403 => ("FORBIDDEN", "forbidden"),
        409 => ("SESSION_BUSY", "session busy"),
        429 => ("RATE_LIMITED", "too many requests"),
        503 => ("SERVICE_UNAVAILABLE", "service unavailable"),
        504 => ("TIMEOUT", "gateway timeout"),
        _ => ("REMOTE_ERROR", "request failed"),
    };
    let code = body
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or(default_code);
    let message = body
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or(default_message);
    ClientError::Remote {
        code: code.to_string(),
        message: message.to_string(),
    }
}

/// Status-only fallback for transports without a JSON body (SSE failures).
pub fn from_status(status: u16) -> ClientError {
    from_status_body(status, &serde_json::Value::Null)
}

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
        P::ErrorResponse(rv) => {
            let status = rv.status().as_u16();
            ClientError::Remote {
                code: status_remote_code(status).to_string(),
                message: format!("error response: {:?}", rv.into_inner()),
            }
        }
        P::ResponseBodyError(err) => ClientError::Invalid(err.to_string()),
        P::InvalidResponsePayload(_, err) => ClientError::Invalid(err.to_string()),
        P::UnexpectedResponse(resp) => {
            let status = resp.status().as_u16();
            if status == 401 {
                ClientError::Unauthorized
            } else if status == 404 {
                ClientError::NotFound(status.to_string())
            } else if status == 403
                || status == 409
                || status == 429
                || status == 503
                || status == 504
            {
                ClientError::Remote {
                    code: status_remote_code(status).to_string(),
                    message: format!("unexpected HTTP {status}"),
                }
            } else {
                ClientError::Http(format!("unexpected HTTP {status}"))
            }
        }
        P::Custom(msg) => ClientError::Http(msg),
    }
}

/// Map a typed progenitor error carrying `ErrorResponse` while preserving
/// the server-sent code and message. Stable 404 codes stay in `Remote`
/// so directory-vs-entity mismatches remain distinguishable.
pub fn map_typed_error(err: progenitor_client::Error<crate::models::ErrorResponse>) -> ClientError {
    use progenitor_client::Error as P;
    match err {
        P::ErrorResponse(rv) => {
            let status = rv.status().as_u16();
            let body = rv.into_inner();
            if status == 401 {
                return ClientError::Unauthorized;
            }
            if status == 404 {
                match body.error.code.as_str() {
                    "SESSION_NOT_FOUND" | "LOCATION_NOT_FOUND" | "TICKET_INVALID"
                    | "SESSION_BUSY" => {
                        return ClientError::Remote {
                            code: body.error.code,
                            message: body.error.message,
                        };
                    }
                    _ => return ClientError::NotFound(body.error.message),
                }
            }
            ClientError::Remote {
                code: body.error.code,
                message: body.error.message,
            }
        }
        other => map_progenitor(other),
    }
}

fn status_remote_code(status: u16) -> &'static str {
    match status {
        403 => "FORBIDDEN",
        404 => "NOT_FOUND",
        409 => "SESSION_BUSY",
        429 => "RATE_LIMITED",
        503 => "SERVICE_UNAVAILABLE",
        504 => "TIMEOUT",
        _ => "REMOTE_ERROR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middleware_statuses_keep_their_codes() {
        for (status, code) in [
            (403, "FORBIDDEN"),
            (429, "RATE_LIMITED"),
            (503, "SERVICE_UNAVAILABLE"),
            (504, "TIMEOUT"),
        ] {
            let err = from_status_body(status, &serde_json::Value::Null);
            assert!(matches!(err, ClientError::Remote { code: ref c, .. } if c == code));
        }
        assert!(matches!(
            from_status_body(401, &serde_json::Value::Null),
            ClientError::Unauthorized
        ));
    }

    #[test]
    fn envelope_code_beats_status_default() {
        let body = serde_json::json!({
            "success": false,
            "data": null,
            "error": {"code": "NOT_FOUND", "message": "missing"}
        });
        let err = from_status_body(404, &body);
        assert!(matches!(err, ClientError::NotFound(m) if m == "missing"));
        let err = from_status_body(500, &body);
        assert!(matches!(err, ClientError::Remote { code, .. } if code == "NOT_FOUND"));
    }

    #[test]
    fn stable_codes_survive_verbatim() {
        for (status, code) in [
            (403u16, "TICKET_INVALID"),
            (404u16, "SESSION_NOT_FOUND"),
            (404u16, "LOCATION_NOT_FOUND"),
            (409u16, "SESSION_BUSY"),
        ] {
            let body = serde_json::json!({
                "success": false,
                "data": null,
                "error": {"code": code, "message": "stable"}
            });
            let err = from_status_body(status, &body);
            assert!(
                matches!(err, ClientError::Remote { code: ref c, .. } if c == code),
                "stable code {code} must survive"
            );
        }
        let err = from_status_body(409, &serde_json::Value::Null);
        assert!(matches!(err, ClientError::Remote { code, .. } if code == "SESSION_BUSY"));
    }
}
