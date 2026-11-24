//! Manual compaction route: `POST /agent-loops/{id}/compact`.
//!
//! Drives the compaction engine bound to a live agent loop through the shared
//! `wf-api::compaction` face. Ownership is checked like every other
//! execution control route; a missing loop is a not-found error.

use axum::extract::{Extension, Path, State};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

/// Wire body of `POST /agent-loops/{id}/compact`.
#[derive(Deserialize, ToSchema, Default)]
pub struct CompactBody {
    /// Optional free-form reason recorded in the audit trail.
    #[serde(default)]
    pub reason: String,
}

/// Wire response of a manual compaction. Mirrors the shared face result so
/// the OpenAPI schema can be derived here without pulling `utoipa` into the
/// API crate.
#[derive(Serialize, ToSchema)]
pub struct CompactResponse {
    pub applied: bool,
    pub kind: String,
    pub engine: Option<String>,
    pub detail: String,
}

impl From<wf_api::compaction::CompactResult> for CompactResponse {
    fn from(result: wf_api::compaction::CompactResult) -> Self {
        Self {
            applied: result.applied,
            kind: result.kind,
            engine: result.engine,
            detail: result.detail,
        }
    }
}

pub(crate) fn routes() -> Router<ApiState> {
    Router::new().route("/agent-loops/{id}/compact", post(handle_compact))
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/compact",
    tag = "agent",
    params(IdPath),
    request_body = CompactBody,
    responses(
        (status = 200, description = "Manual compaction result", body = crate::envelope::ApiEnvelope<CompactResponse>),
        (status = 400, description = "Invalid request", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_compact(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
    Json(body): Json<CompactBody>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_agent_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    let params = wf_api::compaction::CompactParams {
        loop_id: path.id,
        reason: body.reason,
    };
    match wf_api::compaction::compact_now(&state.ctx, params).await {
        Ok(result) => ok(CompactResponse::from(result)).into_response(),
        Err(e) => error_response(e),
    }
}
