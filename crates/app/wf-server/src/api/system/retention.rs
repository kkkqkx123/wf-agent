//! Retention settings and execution lifecycle transport routes.

use axum::extract::{Extension, Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::ToSchema;
use wf_types::config::RetentionConfig;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/system/retention",
            get(handle_get_retention).put(handle_put_retention),
        )
        .route("/system/retention/scan", post(handle_scan_retention))
        .route("/executions/{id}/lifecycle", get(handle_get_lifecycle))
        .route(
            "/executions/{id}/favorite",
            put(handle_favorite_execution).delete(handle_unfavorite_execution),
        )
        .route(
            "/executions/{id}/favorite/refresh",
            post(handle_refresh_favorite),
        )
        .route("/executions/{id}/archive", post(handle_archive_execution))
        .route(
            "/executions/{id}/preserved-bundle/restore",
            post(handle_restore_preserved),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ScanRetentionBody {
    #[serde(default = "default_dry_run")]
    dry_run: bool,
}

fn default_dry_run() -> bool {
    true
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RestorePreservedBody {
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    dry_run: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/system/retention",
    operation_id = "get_system_retention",
    tag = "retention",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_retention(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::retention::settings::current(&state.ctx).await {
        Ok(config) => ok(config).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/system/retention",
    operation_id = "put_system_retention",
    tag = "retention",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_put_retention(
    State(state): State<ApiState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let config: RetentionConfig = match serde_json::from_value(body) {
        Ok(config) => config,
        Err(e) => {
            return error_response(wf_api::ApiError::Validation(format!(
                "invalid retention settings: {e}"
            )))
        }
    };
    match wf_api::retention::settings::replace(&state.ctx, config).await {
        Ok(config) => ok(config).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/system/retention/scan",
    operation_id = "post_system_retention_scan",
    tag = "retention",
    request_body = ScanRetentionBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_scan_retention(
    State(state): State<ApiState>,
    Json(body): Json<ScanRetentionBody>,
) -> impl IntoResponse {
    match wf_api::retention::scan(&state.ctx, body.dry_run).await {
        Ok(report) => ok(report).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/lifecycle",
    operation_id = "get_executions_id_lifecycle",
    tag = "retention",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_lifecycle(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::retention::lifecycle::get(&state.ctx, &path.id).await {
        Ok(entry) => ok(entry).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/executions/{id}/favorite",
    operation_id = "put_executions_id_favorite",
    tag = "retention",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_favorite_execution(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::retention::lifecycle::preserve(&state.ctx, &path.id).await {
        Ok(entry) => ok(entry).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/executions/{id}/favorite",
    operation_id = "delete_executions_id_favorite",
    tag = "retention",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unfavorite_execution(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::retention::lifecycle::unpreserve(&state.ctx, &path.id).await {
        Ok(removed) => ok(removed).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/favorite/refresh",
    operation_id = "post_executions_id_favorite_refresh",
    tag = "retention",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_refresh_favorite(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::retention::lifecycle::refresh(&state.ctx, &path.id).await {
        Ok(entry) => ok(entry).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/archive",
    operation_id = "post_executions_id_archive",
    tag = "retention",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 409, description = "Execution is live", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_archive_execution(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::retention::archive_execution(&state.ctx, &path.id).await {
        Ok(entry) => ok(entry).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/preserved-bundle/restore",
    operation_id = "post_executions_id_preserved_bundle_restore",
    tag = "retention",
    params(IdPath),
    request_body = RestorePreservedBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_restore_preserved(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
    Json(body): Json<RestorePreservedBody>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::retention::lifecycle::restore(&state.ctx, &path.id, body.replace, body.dry_run)
        .await
    {
        Ok(plan) => ok(plan).into_response(),
        Err(e) => error_response(e),
    }
}
