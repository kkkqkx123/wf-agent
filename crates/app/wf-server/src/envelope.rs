//! Response envelope and error mapping shared by the metrics and API
//! routers: every handler responds through `ok` / `err` / `error_response`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

pub(crate) struct ApiError {
    message: String,
}

impl ApiError {
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ApiErrorBody {
    code: String,
    message: String,
    /// Stable engine-error category (`validation`, `not_found`, ...).
    /// Absent for transport-layer errors (auth middleware, rate limiting,
    /// local parameter parsing) that carry no engine classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
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
                category: None,
            },
        }
    }
}

/// Single mapping from an engine-error category onto the HTTP status and
/// stable code. Every engine error renders through this table; only the
/// `internal` category reports a 500.
fn category_status(category: wf_api::ApiErrorCategory) -> (StatusCode, &'static str) {
    use wf_api::ApiErrorCategory as Category;
    match category {
        Category::Validation => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
        Category::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        Category::Conflict => (StatusCode::CONFLICT, "CONFLICT"),
        Category::Cancelled => (StatusCode::CONFLICT, "CANCELLED"),
        Category::Timeout => (StatusCode::GATEWAY_TIMEOUT, "TIMEOUT"),
        Category::BusinessFailure => (StatusCode::UNPROCESSABLE_ENTITY, "NODE_EXECUTION_FAILED"),
        Category::Resource => (StatusCode::TOO_MANY_REQUESTS, "RESOURCE_EXHAUSTED"),
        Category::ServiceUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE"),
        Category::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
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
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new("INVALID_PARAMS", e.message)),
    )
}

/// Map a `wf-api` error onto the response envelope used by the metrics and
/// API routers.
pub(crate) fn api_error_response_internal(
    e: wf_api::ApiError,
) -> (StatusCode, Json<ErrorResponse>) {
    use wf_api::ApiError;
    use wf_api::ApiErrorCategory as Category;
    let (status, code, message, category): (StatusCode, &'static str, String, Option<Category>) =
        match &e {
            ApiError::NotFound { entity_type, id } => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                format!("{entity_type} [{id}] not found"),
                Some(Category::NotFound),
            ),
            ApiError::ExecutionNotFound { id } => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                format!("execution [{id}] not found"),
                Some(Category::NotFound),
            ),
            ApiError::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                msg.clone(),
                Some(Category::Validation),
            ),
            ApiError::AlreadyExists { entity_type, id } => (
                StatusCode::CONFLICT,
                "ALREADY_EXISTS",
                format!("{entity_type} [{id}] already exists"),
                Some(Category::Conflict),
            ),
            ApiError::Conflict(msg) => (
                StatusCode::CONFLICT,
                "CONFLICT",
                msg.clone(),
                Some(Category::Conflict),
            ),
            ApiError::Timeout(msg) => (
                StatusCode::GATEWAY_TIMEOUT,
                "TIMEOUT",
                msg.clone(),
                Some(Category::Timeout),
            ),
            ApiError::Storage(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "STORAGE_ERROR",
                err.to_string(),
                Some(Category::Internal),
            ),
            ApiError::Execution {
                message, category, ..
            } => {
                let (status, code) = category_status(*category);
                (status, code, message.clone(), Some(*category))
            }
        };
    let mut response = ErrorResponse::new(code, message);
    response.error.category = category.map(|category| category.as_str().to_string());
    (status, Json(response))
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

#[cfg(test)]
mod tests {
    use super::*;

    use wf_types::workflow::error_branch::NodeErrorCategory;

    fn node_failure(category: NodeErrorCategory) -> wf_api::WorkflowError {
        wf_api::WorkflowError::NodeFailure {
            node_id: "n".to_string(),
            category,
            detail: "detail".to_string(),
        }
    }

    fn assert_error(err: wf_api::ApiError, status: StatusCode, code: &str, category: &str) {
        let (actual_status, body) = api_error_response_internal(err);
        assert_eq!(actual_status, status);
        let body = serde_json::to_value(body.0).expect("error response must serialize");
        assert_eq!(body["success"], false);
        assert_eq!(body["error"]["code"], code);
        assert_eq!(body["error"]["category"], category);
    }

    #[test]
    fn agent_validation_maps_to_400() {
        assert_error(
            wf_api::ApiError::from(wf_api::AgentError::Validation("bad cap".to_string())),
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "validation",
        );
    }

    #[test]
    fn workflow_config_error_maps_to_400() {
        assert_error(
            wf_api::ApiError::from(wf_api::WorkflowError::ConfigError {
                node_id: "n".to_string(),
                field: "model".to_string(),
                detail: "missing".to_string(),
            }),
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "validation",
        );
    }

    #[test]
    fn llm_config_error_maps_to_400() {
        assert_error(
            wf_api::ApiError::from(wf_api::LlmError::ConfigError("bad key".to_string())),
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "validation",
        );
    }

    #[test]
    fn llm_profile_not_found_maps_to_404() {
        assert_error(
            wf_api::ApiError::from(wf_api::LlmError::ProfileNotFound("p".to_string())),
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "not_found",
        );
    }

    #[test]
    fn workflow_handler_not_found_maps_to_404() {
        assert_error(
            wf_api::ApiError::from(wf_api::WorkflowError::HandlerNotFound {
                node_type: "llm".to_string(),
            }),
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "not_found",
        );
    }

    #[test]
    fn illegal_transition_maps_to_409_conflict() {
        assert_error(
            wf_api::ApiError::from(wf_api::AgentError::IllegalStateTransition(
                "bad resume".to_string(),
            )),
            StatusCode::CONFLICT,
            "CONFLICT",
            "conflict",
        );
    }

    #[test]
    fn agent_cancelled_maps_to_409_cancelled() {
        assert_error(
            wf_api::ApiError::from(wf_api::AgentError::Cancelled("stop".to_string())),
            StatusCode::CONFLICT,
            "CANCELLED",
            "cancelled",
        );
    }

    #[test]
    fn agent_execution_timeout_maps_to_504() {
        assert_error(
            wf_api::ApiError::from(wf_api::AgentError::ExecutionTimeout("slow".to_string())),
            StatusCode::GATEWAY_TIMEOUT,
            "TIMEOUT",
            "timeout",
        );
    }

    #[test]
    fn transport_timeout_node_failure_maps_to_504() {
        assert_error(
            wf_api::ApiError::from(node_failure(NodeErrorCategory::TransportTimeout)),
            StatusCode::GATEWAY_TIMEOUT,
            "TIMEOUT",
            "timeout",
        );
    }

    #[test]
    fn node_execution_failed_maps_to_422() {
        assert_error(
            wf_api::ApiError::from(wf_api::WorkflowError::NodeExecutionFailed {
                node_id: "n".to_string(),
                reason: "boom".to_string(),
            }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "NODE_EXECUTION_FAILED",
            "business_failure",
        );
    }

    #[test]
    fn business_failure_node_failure_maps_to_422() {
        assert_error(
            wf_api::ApiError::from(node_failure(NodeErrorCategory::BusinessFailure)),
            StatusCode::UNPROCESSABLE_ENTITY,
            "NODE_EXECUTION_FAILED",
            "business_failure",
        );
    }

    #[test]
    fn tripped_error_pattern_maps_to_422() {
        assert_error(
            wf_api::ApiError::from(wf_api::AgentError::ErrorPatternTripped(
                "Timeout recurred 3 times".to_string(),
            )),
            StatusCode::UNPROCESSABLE_ENTITY,
            "NODE_EXECUTION_FAILED",
            "business_failure",
        );
    }

    #[test]
    fn shared_wrapped_business_failure_maps_to_422() {
        assert_error(
            wf_api::ApiError::from(wf_api::AgentError::SharedError(
                wf_api::ExecutionSharedError::NodeFailure {
                    node_id: "n".to_string(),
                    category: NodeErrorCategory::BusinessFailure,
                    detail: "boom".to_string(),
                },
            )),
            StatusCode::UNPROCESSABLE_ENTITY,
            "NODE_EXECUTION_FAILED",
            "business_failure",
        );
    }

    #[test]
    fn concurrency_saturated_maps_to_429() {
        assert_error(
            wf_api::ApiError::from(wf_api::AgentError::ConcurrencySaturated(
                "gate full".to_string(),
            )),
            StatusCode::TOO_MANY_REQUESTS,
            "RESOURCE_EXHAUSTED",
            "resource",
        );
    }

    #[test]
    fn provider_503_maps_to_service_unavailable() {
        assert_error(
            wf_api::ApiError::from(wf_api::LlmError::ProviderError(
                "HTTP 503 unavailable".to_string(),
            )),
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            "service_unavailable",
        );
    }

    #[test]
    fn agent_internal_maps_to_500() {
        assert_error(
            wf_api::ApiError::from(wf_api::AgentError::Internal("boom".to_string())),
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "internal",
        );
    }

    #[test]
    fn llm_timeout_maps_to_504() {
        assert_error(
            wf_api::ApiError::from(wf_api::LlmError::Timeout(5000)),
            StatusCode::GATEWAY_TIMEOUT,
            "TIMEOUT",
            "timeout",
        );
    }
}
