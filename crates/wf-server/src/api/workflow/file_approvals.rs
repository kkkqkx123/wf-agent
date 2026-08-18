//! File-checkpoint layered approval endpoints: pending approvals and the
//! approve / reject actions of the host-side approval flow (approval policy
//! `manual` — "review after the run ends"). Handlers are thin transport
//! adapters over `wf-api::workflow::file_approval`.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Deserialize)]
struct ApproveRequest {
    #[serde(default)]
    feature: String,
}

#[derive(Debug, Serialize)]
struct RejectResponse {
    baseline_snapshot_id: String,
}

async fn handle_list_pending_approvals(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::workflow::file_approval::list_pending_approvals(&state.ctx) {
        Ok(approvals) => ok(approvals).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_approve_changes(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    body: Option<axum::extract::Json<ApproveRequest>>,
) -> impl IntoResponse {
    let feature = body.map(|b| b.0.feature).unwrap_or_default();
    match wf_api::workflow::file_approval::approve_changes(&state.ctx, &path.id, &feature) {
        Ok(outcome) => ok(outcome).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_reject_changes(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::file_approval::reject_changes(&state.ctx, &path.id) {
        Ok(baseline_snapshot_id) => ok(RejectResponse {
            baseline_snapshot_id,
        })
        .into_response(),
        Err(err) => error_response(err),
    }
}
