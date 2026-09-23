//! Workflow version surface: version listing / retrieval / save and
//! rollback. Split from `api_workflows` to keep the workflow surface at a
//! maintainable file size.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use wf_api::WorkflowDefinition;

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
        .route(
            "/workflows/{id}/versions/increment",
            post(handle_increment_version),
        )
        .route("/workflows/{id}/rollback", post(handle_rollback_workflow))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/versions",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_versions(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::list_workflow_versions(&state.ctx, &path.id).await {
        Ok(versions) => ok(versions).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/versions/{version}",
    tag = "workflow",
    params(IdVersionPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_version(
    State(state): State<ApiState>,
    Path(path): Path<IdVersionPath>,
) -> impl IntoResponse {
    match wf_api::workflow::get_workflow_version(&state.ctx, &path.id, &path.version).await {
        Ok(version) => ok(version).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SaveVersionBody {
    version: String,
    #[schema(value_type = Object)]
    workflow: WorkflowDefinition,
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/versions",
    tag = "workflow",
    params(IdPath),
    request_body = SaveVersionBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_version(
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

#[derive(Deserialize, ToSchema)]
pub(crate) struct RollbackBody {
    version: String,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct IncrementVersionQuery {
    level: Option<String>,
}

#[derive(Deserialize, Default, ToSchema)]
#[serde(default)]
pub(crate) struct IncrementVersionBody {
    #[schema(value_type = Object)]
    changes: wf_api::workflow::WorkflowChanges,
    keep_original: bool,
}

/// Semantic-version increment through `wf-api::create_versioned_update`:
/// applies optional field-level changes and bumps the version, optionally
/// preserving the pre-update definition as a named version.
#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/versions/increment",
    tag = "workflow",
    params(IdPath, IncrementVersionQuery),
    request_body = IncrementVersionBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_increment_version(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<IncrementVersionQuery>,
    Json(body): Json<IncrementVersionBody>,
) -> impl IntoResponse {
    let strategy = match query.level.as_deref() {
        None | Some("patch") => wf_api::workflow::VersionStrategy::Patch,
        Some("minor") => wf_api::workflow::VersionStrategy::Minor,
        Some("major") => wf_api::workflow::VersionStrategy::Major,
        Some(other) => {
            return crate::envelope::err(crate::envelope::ApiError::validation(format!(
                "unsupported version level: {other}"
            )))
            .into_response()
        }
    };
    match wf_api::workflow::create_versioned_update(
        &state.ctx,
        &path.id,
        strategy,
        &body.changes,
        body.keep_original,
    )
    .await
    {
        Ok(version) => ok(version).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/rollback",
    tag = "workflow",
    params(IdPath),
    request_body = RollbackBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rollback_workflow(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<RollbackBody>,
) -> impl IntoResponse {
    match wf_api::workflow::rollback_workflow(&state.ctx, &path.id, &body.version).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}
