//! File-checkpoint layered approval endpoints: pending approvals and the
//! approve / reject actions of the host-side approval flow (approval policy
//! `manual` — "review after the run ends"). Handlers are thin transport
//! adapters over `wf-api::workflow::file_approval`.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/file-checkpoint/approvals/pending",
            get(handle_list_pending_approvals),
        )
        .route(
            "/file-checkpoint/approvals/{id}/approve",
            post(handle_approve_changes),
        )
        .route(
            "/file-checkpoint/approvals/{id}/reject",
            post(handle_reject_changes),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ApproveRequest {
    /// Target feature branch for approval
    #[serde(default)]
    feature: String,
    /// File-level approval: when set and non-empty, only these paths are
    /// advanced into the feature; the rest stay pending.
    #[serde(default)]
    paths: Option<Vec<String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct RejectResponse {
    baseline_snapshot_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RejectRequest {
    /// Optional human-readable rejection reason
    #[serde(default)]
    reason: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/approvals/pending",
    tag = "checkpoint",
    responses(
        (status = 200, description = "List of pending file approvals", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_pending_approvals(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    match wf_api::checkpoint::approval::list_pending_approvals(&state.ctx) {
        Ok(approvals) => ok(approvals).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/approvals/{id}/approve",
    tag = "checkpoint",
    params(IdPath),
    request_body = ApproveRequest,
    responses(
        (status = 200, description = "Changes approved", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_approve_changes(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    body: Option<axum::extract::Json<ApproveRequest>>,
) -> impl IntoResponse {
    let (feature, paths) = body.map(|b| (b.0.feature, b.0.paths)).unwrap_or_default();
    match wf_api::checkpoint::approval::approve_changes(&state.ctx, &path.id, &feature, paths) {
        Ok(outcome) => ok(outcome).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/approvals/{id}/reject",
    tag = "checkpoint",
    params(IdPath),
    request_body = RejectRequest,
    responses(
        (status = 200, description = "Changes rejected", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::file_approvals::RejectResponse>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_reject_changes(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    body: Option<axum::extract::Json<RejectRequest>>,
) -> impl IntoResponse {
    let reason = body.and_then(|b| b.0.reason);
    match wf_api::checkpoint::approval::reject_changes(&state.ctx, &path.id, reason.as_deref()) {
        Ok(baseline_snapshot_id) => ok(RejectResponse {
            baseline_snapshot_id,
        })
        .into_response(),
        Err(err) => error_response(err),
    }
}
