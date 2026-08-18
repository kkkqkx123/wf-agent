//! Response envelope and error mapping shared by the metrics and API
//! routers: every handler responds through `ok` / `err` / `error_response`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Error classification used by the response envelope. The full mapping
/// (NotFound -> 404, Validation -> 400, anything else -> 500) is the API
/// contract; today only Validation is reachable from the handlers.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    NotFound,
    Validation,
    Internal,
}

impl ErrorKind {
    fn status(self) -> StatusCode {
        match self {
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Validation => StatusCode::BAD_REQUEST,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(self) -> &'static str {
        match self {
            ErrorKind::NotFound => "NOT_FOUND",
            ErrorKind::Validation => "INVALID_PARAMS",
            ErrorKind::Internal => "INTERNAL_ERROR",
        }
    }
}

pub(crate) struct ApiError {
    kind: ErrorKind,
    message: String,
}

impl ApiError {
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Validation,
            message: message.into(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ApiErrorBody {
    code: String,
    message: String,
}

#[derive(Serialize)]
pub(crate) struct ApiEnvelope<T: Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<ApiErrorBody>,
}

pub(crate) fn ok<T: Serialize>(data: T) -> Json<ApiEnvelope<T>> {
    Json(ApiEnvelope {
        success: true,
        data: Some(data),
        error: None,
    })
}

pub(crate) fn err<T: Serialize>(e: ApiError) -> (StatusCode, Json<ApiEnvelope<T>>) {
    (
        e.kind.status(),
        Json(ApiEnvelope {
            success: false,
            data: None,
            error: Some(ApiErrorBody {
                code: e.kind.code().to_string(),
                message: e.message,
            }),
        }),
    )
}

/// Map a `wf-api` error onto the response envelope used by the metrics and
/// API routers.
pub(crate) fn api_error_response_internal(
    e: wf_api::ApiError,
) -> (StatusCode, Json<ApiEnvelope<ApiErrorBody>>) {
    use wf_api::ApiError;
    let (status, code, message) = match &e {
        ApiError::NotFound { entity_type, id } => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("{entity_type} [{id}] not found"),
        ),
        ApiError::ExecutionNotFound { id } => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("execution [{id}] not found"),
        ),
        ApiError::Validation(msg) => (StatusCode::BAD_REQUEST, "INVALID_PARAMS", msg.clone()),
        ApiError::AlreadyExists { entity_type, id } => (
            StatusCode::CONFLICT,
            "ALREADY_EXISTS",
            format!("{entity_type} [{id}] already exists"),
        ),
        ApiError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg.clone()),
        ApiError::Timeout(msg) => (StatusCode::GATEWAY_TIMEOUT, "TIMEOUT", msg.clone()),
        ApiError::Storage(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "STORAGE_ERROR",
            err.to_string(),
        ),
        ApiError::Execution { message, .. } => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            message.clone(),
        ),
    };
    (
        status,
        Json::<ApiEnvelope<ApiErrorBody>>(ApiEnvelope {
            success: false,
            data: None,
            error: Some(ApiErrorBody {
                code: code.to_string(),
                message,
            }),
        }),
    )
}

/// Render a `wf-api` error through the envelope.
pub(crate) fn error_response(e: wf_api::ApiError) -> Response {
    api_error_response_internal(e).into_response()
}

/// 401 response used by the auth middleware (and, later, by handlers).
pub(crate) fn unauthorized(message: impl Into<String>) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json::<ApiEnvelope<ApiErrorBody>>(ApiEnvelope {
            success: false,
            data: None,
            error: Some(ApiErrorBody {
                code: "UNAUTHORIZED".to_string(),
                message: message.into(),
            }),
        }),
    )
        .into_response()
}

/// 403 response used by the auth middleware.
pub(crate) fn forbidden(message: impl Into<String>) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json::<ApiEnvelope<ApiErrorBody>>(ApiEnvelope {
            success: false,
            data: None,
            error: Some(ApiErrorBody {
                code: "FORBIDDEN".to_string(),
                message: message.into(),
            }),
        }),
    )
        .into_response()
}

/// 429 response used by the rate-limit middleware.
pub(crate) fn rate_limited(retry_after_secs: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json::<ApiEnvelope<ApiErrorBody>>(ApiEnvelope {
            success: false,
            data: None,
            error: Some(ApiErrorBody {
                code: "RATE_LIMITED".to_string(),
                message: "Too many requests, please slow down.".to_string(),
            }),
        }),
    )
        .into_response();
    if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after_secs.to_string()) {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, value);
    }
    response
}

/// 503 response used when a resource limit is reached (e.g. max concurrent
/// SSE connections).
pub(crate) fn service_unavailable(message: impl Into<String>) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json::<ApiEnvelope<ApiErrorBody>>(ApiEnvelope {
            success: false,
            data: None,
            error: Some(ApiErrorBody {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: message.into(),
            }),
        }),
    )
        .into_response()
}
