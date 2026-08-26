//! Workflow version surface: version listing / retrieval / save and
//! rollback. Split from `api_workflows` to keep the workflow surface at a
//! maintainable file size.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use wf_types::WorkflowDefinition;

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, IdVersionPath};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/workflows/{id}/versions",
            get(handle_list_versions).post(handle_save_version),
        )
        .route(
            "/workflows/{id}/versions/{version}",
            get(handle_get_version),
        )
        .route("/workflows/{id}/rollback", post(handle_rollback_workflow))
}

async fn handle_list_versions(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::list_workflow_versions(&state.ctx, &path.id).await {
        Ok(versions) => ok(versions).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_version(
    State(state): State<ApiState>,
    Path(path): Path<IdVersionPath>,
) -> impl IntoResponse {
    match wf_api::workflow::get_workflow_version(&state.ctx, &path.id, &path.version).await {
        Ok(version) => ok(version).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SaveVersionBody {
    version: String,
    workflow: WorkflowDefinition,
}

async fn handle_save_version(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<SaveVersionBody>,
) -> impl IntoResponse {
    match wf_api::workflow::save_workflow_version(
        &state.ctx,
        &path.id,
        &body.version,
        &body.workflow,
    )
    .await
    {
        Ok(()) => ok(body.version).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct RollbackBody {
    version: String,
}

async fn handle_rollback_workflow(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<RollbackBody>,
) -> impl IntoResponse {
    match wf_api::workflow::rollback_workflow(&state.ctx, &path.id, &body.version).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}
