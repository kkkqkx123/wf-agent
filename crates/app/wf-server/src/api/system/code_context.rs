//! Code-context surface: the read-only status view plus the maintenance
//! actions over a broken index.
//!
//! The status view is assembled by `wf-api` from cached live state, so the
//! read path never probes, never mutates the breaker and never makes an
//! outbound call. The maintenance actions (dead-letter read, retry,
//! acknowledge, manual reindex) do reach the service: they are operator
//! actions, deliberately explicit instead of automatic.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/system/code-context", get(handle_code_context))
        .route(
            "/system/code-context/dead-letters",
            get(handle_dead_letters),
        )
        .route(
            "/system/code-context/dead-letters/retry",
            post(handle_dead_letters_retry),
        )
        .route(
            "/system/code-context/dead-letters/acknowledge",
            post(handle_dead_letters_acknowledge),
        )
        .route(
            "/system/code-context/reindex",
            post(handle_code_context_reindex),
        )
}

/// Body of `POST /api/v1/system/code-context/dead-letters/retry`.
///
/// Paths are project-relative as the service records them. An empty list
/// retries every candidate.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub(crate) struct DeadLetterRetryBody {
    /// Files to retry; empty retries every dead letter.
    #[serde(default)]
    pub files: Vec<String>,
}

/// Body of `POST /api/v1/system/code-context/dead-letters/acknowledge`.
///
/// Acknowledging stops the retry passes for one module of one file: it is the
/// operator saying "this failure is accepted", not a repair.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct DeadLetterAcknowledgeBody {
    /// Recorded file path of the dead letter.
    pub file_path: String,
    /// Module to acknowledge (relation / summary / embedding / bm25 / export).
    /// Omitted acknowledges every module of the file.
    #[serde(default)]
    pub module: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/system/code-context",
    tag = "system",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_code_context(State(state): State<ApiState>) -> impl IntoResponse {
    let diagnostic = wf_api::infra::diagnostics::code_context(&state.ctx);
    ok(serde_json::json!(diagnostic))
}

#[utoipa::path(
    get,
    path = "/api/v1/system/code-context/dead-letters",
    tag = "system",
    responses(
        (status = 200, description = "Dead-lettered files of the workspace project", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 422, description = "Service not attached or maintenance rejected", body = crate::envelope::ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_dead_letters(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::infra::code_context::dead_letters(&state.ctx).await {
        Ok(list) => ok(serde_json::json!(list)).into_response(),
        Err(error) => error_response(error),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/system/code-context/dead-letters/retry",
    tag = "system",
    request_body = DeadLetterRetryBody,
    responses(
        (status = 200, description = "Retry outcome", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 422, description = "Service not attached or maintenance rejected", body = crate::envelope::ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_dead_letters_retry(
    State(state): State<ApiState>,
    Json(body): Json<DeadLetterRetryBody>,
) -> impl IntoResponse {
    match wf_api::infra::code_context::retry_dead_letters(&state.ctx, body.files).await {
        Ok(action) => ok(serde_json::json!(action)).into_response(),
        Err(error) => error_response(error),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/system/code-context/dead-letters/acknowledge",
    tag = "system",
    request_body = DeadLetterAcknowledgeBody,
    responses(
        (status = 200, description = "Acknowledge outcome", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 422, description = "Service not attached or maintenance rejected", body = crate::envelope::ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_dead_letters_acknowledge(
    State(state): State<ApiState>,
    Json(body): Json<DeadLetterAcknowledgeBody>,
) -> impl IntoResponse {
    match wf_api::infra::code_context::acknowledge_dead_letter(
        &state.ctx,
        body.file_path,
        body.module,
    )
    .await
    {
        Ok(action) => ok(serde_json::json!(action)).into_response(),
        Err(error) => error_response(error),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/system/code-context/reindex",
    tag = "system",
    responses(
        (status = 200, description = "Updated status view after the reindex was queued", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 422, description = "Service not attached or reindex rejected", body = crate::envelope::ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_code_context_reindex(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    match wf_api::infra::code_context::retry_index(&state.ctx).await {
        Ok(diagnostic) => ok(serde_json::json!(diagnostic)).into_response(),
        Err(error) => error_response(error),
    }
}
