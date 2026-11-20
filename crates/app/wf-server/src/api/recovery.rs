//! Restoration of execution recovery bundles on an execution node. The route
//! is registered only when recovery writes are explicitly enabled.

use axum::extract::{Extension, State};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use wf_api::execution_bundle::{restore_bundle, ExecutionBundle, RestoreMode};

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct RestoreRequest {
    pub(crate) bundle: ExecutionBundle,
    pub(crate) dry_run: bool,
    /// Overwrite permission for a target that already holds different
    /// content. Defaults to refusing, so a caller must ask for a replace.
    #[serde(default)]
    pub(crate) mode: RestoreMode,
}

pub(crate) fn routes() -> Router<ApiState> {
    if std::env::var("WF_RECOVERY_RESTORE_ENABLED").as_deref() != Ok("1") {
        return Router::new();
    }
    Router::new().route(
        "/remote-sync/executions/restore",
        post(handle_restore_bundle),
    )
}

#[utoipa::path(
    post,
    path = "/api/v1/remote-sync/executions/restore",
    operation_id = "restore_remote_execution_bundle",
    tag = "observation",
    responses((status = 200, description = "Restore plan or result", body = serde_json::Value), (status = 400, description = "Invalid bundle or conflict", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_restore_bundle(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    axum::Json(request): axum::Json<RestoreRequest>,
) -> impl IntoResponse {
    let scope = match crate::directory::require_scope(&scope) {
        Ok(scope) => scope,
        Err(e) => return error_response(e),
    };
    let directory = scope.directory.to_string_lossy().into_owned();
    match restore_bundle(
        &state.ctx,
        &request.bundle,
        Some(directory.as_str()),
        request.dry_run,
        request.mode,
    )
    .await
    {
        Ok(plan) => ok(plan).into_response(),
        Err(e) => error_response(e),
    }
}
