//! Response envelope and error mapping shared by the metrics and API
//! routers: every handler responds through `ok` / `err` / `error_response`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

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

#[derive(Serialize, ToSchema)]
pub struct ApiErrorBody {
    code: String,
    message: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ApiEnvelope<T: Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<ApiErrorBody>,
}

/// Runtime and documentation envelope for every error response. Success
/// payloads keep using `ApiEnvelope<T>`; errors always carry `data: null`
/// and a populated `error`.
#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    success: bool,
    data: Option<serde_json::Value>,
    error: ApiErrorBody,
}

impl ErrorResponse {
    pub(crate) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: ApiErrorBody {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}

pub(crate) fn ok<T: Serialize>(data: T) -> Json<ApiEnvelope<T>> {
    Json(ApiEnvelope {
        success: true,
        data: Some(data),
        error: None,
    })
}

pub(crate) fn err(e: ApiError) -> (StatusCode, Json<ErrorResponse>) {
    (
        e.kind.status(),
        Json(ErrorResponse::new(e.kind.code(), e.message)),
    )
}

/// Map a `wf-api` error onto the response envelope used by the metrics and
/// API routers.
pub(crate) fn api_error_response_internal(
    e: wf_api::ApiError,
) -> (StatusCode, Json<ErrorResponse>) {
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
    (status, Json(ErrorResponse::new(code, message)))
}

/// Render a `wf-api` error through the envelope.
pub(crate) fn error_response(e: wf_api::ApiError) -> Response {
    api_error_response_internal(e).into_response()
}

/// Render a payload as a file download (`Content-Disposition: attachment`)
/// with content type, filename and length headers so browsers save it
/// directly instead of displaying JSON.
pub(crate) fn download(payload: &str, content_type: &str, filename: &str) -> Response {
    let mut response = payload.to_owned().into_response();
    let headers = response.headers_mut();
    if let Ok(value) = content_type.parse::<axum::http::HeaderValue>() {
        headers.insert(axum::http::header::CONTENT_TYPE, value);
    }
    if let Ok(value) =
        format!("attachment; filename=\"{filename}\"").parse::<axum::http::HeaderValue>()
    {
        headers.insert(axum::http::header::CONTENT_DISPOSITION, value);
    }
    if let Ok(value) = payload.len().to_string().parse::<axum::http::HeaderValue>() {
        headers.insert(axum::http::header::CONTENT_LENGTH, value);
    }
    response
}

/// 401 response used by the auth middleware.
pub(crate) fn unauthorized(message: impl Into<String>) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse::new("UNAUTHORIZED", message)),
    )
        .into_response()
}

/// 403 response used by the auth middleware.
pub(crate) fn forbidden(message: impl Into<String>) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new("FORBIDDEN", message)),
    )
        .into_response()
}

/// 429 response used by the rate-limit middleware.
pub(crate) fn rate_limited(retry_after_secs: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ErrorResponse::new(
            "RATE_LIMITED",
            "Too many requests, please slow down.",
        )),
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
        Json(ErrorResponse::new("SERVICE_UNAVAILABLE", message)),
    )
        .into_response()
}
