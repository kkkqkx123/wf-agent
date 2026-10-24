//! Node template version surface: version listing / retrieval / save and
//! rollback.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, IdVersionPath};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/templates/node/{id}/versions",
            get(handle_list_versions).post(handle_save_version),
        )
        .route(
            "/templates/node/{id}/versions/{version}",
            get(handle_get_version),
        )
        .route("/templates/node/{id}/rollback", post(handle_rollback))
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/node/{id}/versions",
    operation_id = "get_templates_node_id_versions",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Node template version list", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_versions(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::node_version::list_node_template_versions(&state.ctx, &path.id).await {
        Ok(versions) => ok(versions).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/node/{id}/versions/{version}",
    operation_id = "get_templates_node_id_versions_version",
    tag = "template",
    params(IdVersionPath),
    responses((status = 200, description = "Node template version detail", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_version(
    State(state): State<ApiState>,
    Path(path): Path<IdVersionPath>,
) -> impl IntoResponse {
    match wf_api::template::node_version::get_node_template_version(
        &state.ctx,
        &path.id,
        &path.version,
    )
    .await
    {
        Ok(version) => ok(version).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SaveVersionBody {
    version: String,
    #[schema(value_type = Object)]
    template: wf_api::NodeTemplateStorageMetadata,
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/node/{id}/versions",
    operation_id = "post_templates_node_id_versions",
    tag = "template",
    params(IdPath),
    request_body = SaveVersionBody,
    responses((status = 200, description = "Node template version saved", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_version(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<SaveVersionBody>,
) -> impl IntoResponse {
    match wf_api::template::node_version::save_node_template_version(
        &state.ctx,
        &path.id,
        &body.version,
        &body.template,
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

#[utoipa::path(
    post,
    path = "/api/v1/templates/node/{id}/rollback",
    operation_id = "post_templates_node_id_rollback",
    tag = "template",
    params(IdPath),
    request_body = RollbackBody,
    responses((status = 200, description = "Node template rolled back", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rollback(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<RollbackBody>,
) -> impl IntoResponse {
    match wf_api::template::node_version::rollback_node_template(
        &state.ctx,
        &path.id,
        &body.version,
    )
    .await
    {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}
