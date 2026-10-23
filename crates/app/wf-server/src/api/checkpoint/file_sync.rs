//! File-checkpoint sync endpoints: export the main tree into the user
//! workspace, import external commits back as human history, rebase the
//! baseline, and query reconcile/external-update state. Handlers are thin
//! transport adapters over `wf-api::checkpoint::sync`.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/file-checkpoint/sync/export", post(handle_export))
        .route("/file-checkpoint/sync/import", post(handle_import))
        .route("/file-checkpoint/sync/rebase", post(handle_rebase))
        .route("/file-checkpoint/sync/hint", get(handle_hint))
        .route("/file-checkpoint/sync/reconcile", get(handle_reconcile))
        .route("/file-checkpoint/sync/config", get(handle_sync_config))
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ExportRequest {
    /// Must be true when the worktree differs from main; otherwise the
    /// export is rejected instead of overwriting unexported changes.
    confirm_overwrite: bool,
    /// When set, overrides the configured `sync.auto_commit` default.
    #[serde(default)]
    auto_commit: Option<bool>,
    /// Optional commit message; otherwise a message is suggested from the
    /// commits since the last reconcile point.
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RebaseRequest {
    /// `borrow` (default) or `copy`.
    #[serde(default)]
    mode: Option<String>,
    /// User-repository branch the new baseline is resolved from.
    #[serde(default)]
    branch: Option<String>,
    /// Commit hash prefix pinned inside the user repository.
    #[serde(default)]
    commit_prefix: Option<String>,
    /// Nth ancestor back from the branch tip along the first-parent chain.
    #[serde(default)]
    behind: Option<u64>,
    /// Destructive operation guard: must be true.
    confirm: bool,
}

fn rebase_baseline_config(
    body: &RebaseRequest,
) -> Result<wf_types::config::file_checkpoint::BaselineConfig, wf_api::ApiError> {
    if !body.confirm {
        return Err(wf_api::ApiError::Validation(
            "rebase is destructive; set confirm=true".to_string(),
        ));
    }
    let mode = match body.mode.as_deref() {
        None | Some("borrow") => wf_types::config::file_checkpoint::BaselineMode::Borrow,
        Some("copy") => wf_types::config::file_checkpoint::BaselineMode::Copy,
        Some(other) => {
            return Err(wf_api::ApiError::Validation(format!(
                "invalid baseline mode '{other}': expected borrow or copy"
            )));
        }
    };
    Ok(wf_types::config::file_checkpoint::BaselineConfig {
        mode,
        branch: body.branch.clone(),
        commit_prefix: body.commit_prefix.clone(),
        behind: body.behind,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/sync/export",
    tag = "checkpoint",
    request_body = ExportRequest,
    responses(
        (status = 200, description = "Worktree materialized; reconcile point advanced", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::ExportOutcomeDoc>),
        (status = 400, description = "Validation failed (dirty worktree without confirmation, no git workspace)", body = crate::envelope::ErrorResponse),
        (status = 404, description = "Main ref missing", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Export already running or unresolved conflicts", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_export(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<ExportRequest>,
) -> impl IntoResponse {
    let ctx = state.ctx.clone();
    let result = tokio::task::spawn_blocking(move || {
        wf_api::checkpoint::sync::export_to_workspace(
            &ctx,
            body.confirm_overwrite,
            body.auto_commit,
            body.message,
        )
    })
    .await;
    match result {
        Ok(Ok(outcome)) => ok(outcome).into_response(),
        Ok(Err(err)) => error_response(err),
        Err(err) => error_response(wf_api::ApiError::execution(format!(
            "export blocking task failed: {err}"
        ))),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/sync/import",
    tag = "checkpoint",
    responses(
        (status = 200, description = "External head captured as human history; reconcile point advanced", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::ImportOutcomeDoc>),
        (status = 400, description = "No reconcile point or repository mismatch", body = crate::envelope::ErrorResponse),
        (status = 404, description = "Main ref missing", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import(State(state): State<ApiState>) -> impl IntoResponse {
    let ctx = state.ctx.clone();
    let result =
        tokio::task::spawn_blocking(move || wf_api::checkpoint::sync::import_from_external(&ctx))
            .await;
    match result {
        Ok(Ok(outcome)) => ok(outcome).into_response(),
        Ok(Err(err)) => error_response(err),
        Err(err) => error_response(wf_api::ApiError::execution(format!(
            "import blocking task failed: {err}"
        ))),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/sync/rebase",
    tag = "checkpoint",
    request_body = RebaseRequest,
    responses(
        (status = 200, description = "Baseline moved; old main tree transplanted on top", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::BaselineRebaseOutcomeDoc>),
        (status = 400, description = "Missing confirm flag or invalid baseline selector", body = crate::envelope::ErrorResponse),
        (status = 404, description = "No baseline anchor to rebase", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Ref conflict during transplant", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rebase(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<RebaseRequest>,
) -> impl IntoResponse {
    let baseline = match rebase_baseline_config(&body) {
        Ok(baseline) => baseline,
        Err(err) => return error_response(err),
    };
    let ctx = state.ctx.clone();
    let result = tokio::task::spawn_blocking(move || {
        wf_api::checkpoint::sync::rebase_baseline(&ctx, baseline)
    })
    .await;
    match result {
        Ok(Ok(outcome)) => ok(outcome).into_response(),
        Ok(Err(err)) => error_response(err),
        Err(err) => error_response(wf_api::ApiError::execution(format!(
            "rebase blocking task failed: {err}"
        ))),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/sync/hint",
    tag = "checkpoint",
    responses(
        (status = 200, description = "External head ahead of the reconcile point, or null", body = crate::envelope::ApiEnvelope<Option<crate::api::checkpoint::views::ExternalUpdateHintDoc>>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_hint(State(state): State<ApiState>) -> impl IntoResponse {
    let ctx = state.ctx.clone();
    let result =
        tokio::task::spawn_blocking(move || wf_api::checkpoint::sync::external_update_hint(&ctx))
            .await;
    match result {
        Ok(Ok(hint)) => ok(hint).into_response(),
        Ok(Err(err)) => error_response(err),
        Err(err) => error_response(wf_api::ApiError::execution(format!(
            "hint blocking task failed: {err}"
        ))),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/sync/reconcile",
    tag = "checkpoint",
    responses(
        (status = 200, description = "Current reconcile point, or null before the first export", body = crate::envelope::ApiEnvelope<Option<crate::api::checkpoint::views::ReconcilePointDoc>>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_reconcile(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::checkpoint::sync::reconcile_point(&state.ctx) {
        Ok(point) => ok(point).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/sync/config",
    tag = "checkpoint",
    responses(
        (status = 200, description = "Configured sync defaults, or null when unconfigured", body = crate::envelope::ApiEnvelope<Option<crate::api::checkpoint::views::SyncConfigDoc>>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_sync_config(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::checkpoint::sync::sync_config(&state.ctx) {
        Ok(config) => ok(config).into_response(),
        Err(err) => error_response(err),
    }
}
