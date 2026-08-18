//! HTTP transport layer for Layertwine API
//!
//! Provides an axum-based REST/JSON server that wraps the ApiService trait.
//! Enabled with `feature = "http"`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};

use crate::api::service::ApiService;
use crate::api::types::*;

/// Shared application state
struct AppState {
    service: Arc<ApiService>,
}

/// Start the HTTP server
///
/// ```no_run
/// use std::sync::Arc;
/// use layertwine::api::service::{ApiService, ServiceConfig};
/// use layertwine::api::http;
///
/// # async fn example() {
/// let service = ApiService::open(ServiceConfig::default()).unwrap();
/// http::serve(Arc::new(service), "127.0.0.1:8080".parse().unwrap()).await.unwrap();
/// # }
/// ```
pub async fn serve(
    service: Arc<ApiService>,
    addr: SocketAddr,
) -> Result<(), crate::error::LayertwineError> {
    let state = Arc::new(AppState { service });

    let app = Router::new()
        // Repository lifecycle
        .route("/api/v1/init", post(handle_init))
        .route("/api/v1/status", get(handle_status))
        // Edit operations
        .route("/api/v1/edit", post(handle_edit))
        .route("/api/v1/agent/{id}/edit", post(handle_agent_edit))
        .route("/api/v1/agent/{id}/submit", post(handle_agent_submit))
        .route("/api/v1/approve/{agent_id}", post(handle_approve))
        // Grant and approval operations
        .route("/api/v1/approvals", get(handle_list_pending_approvals))
        .route("/api/v1/approve-agent", post(handle_approve_agent))
        .route("/api/v1/reject-agent", post(handle_reject_agent))
        .route("/api/v1/merge-to-unified", post(handle_merge_to_unified))
        .route("/api/v1/merge-to-staged", post(handle_merge_to_staged))
        // Checkpoint operations
        .route("/api/v1/commit", post(handle_commit))
        .route("/api/v1/log", get(handle_log))
        // Branch operations
        .route("/api/v1/branches", get(handle_branch_list))
        .route("/api/v1/branches", post(handle_branch_create))
        .route("/api/v1/branches/{name}/switch", post(handle_branch_switch))
        .route("/api/v1/merge", post(handle_merge))
        // Backup operations
        .route("/api/v1/backup", post(handle_backup))
        .route("/api/v1/restore", post(handle_restore))
        // Maintenance
        .route("/api/v1/gc", post(handle_gc))
        .route("/api/v1/compact", post(handle_compact))
        .route("/api/v1/git-commit", post(handle_git_commit))
        .route("/api/v1/clean", post(handle_clean))
        .route("/api/v1/pull", post(handle_pull))
        .route("/api/v1/show", get(handle_show))
        // Checkpoint restore operations
        .route(
            "/api/v1/checkpoint/restore",
            post(handle_checkpoint_restore),
        )
        .route(
            "/api/v1/checkpoint/restore-by-time",
            post(handle_checkpoint_restore_by_time),
        )
        .route("/api/v1/checkpoint/diff", post(handle_checkpoint_diff))
        .route(
            "/api/v1/checkpoint/rollback",
            post(handle_checkpoint_rollback),
        )
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| crate::error::LayertwineError::General(format!("failed to bind: {}", e)))?;
    axum::serve(listener, app)
        .await
        .map_err(|e| crate::error::LayertwineError::General(format!("server error: {}", e)))
}

// ── Unified response wrapper ──

/// Run a blocking service call on the blocking pool and map the outcome onto
/// the response envelope. `ApiService` performs blocking rusqlite/file I/O, so
/// invoking it directly on an async worker would head-of-line block the server
/// (mirrors the gRPC layer's `spawn_blocking`).
async fn run_blocking<R: serde::Serialize + Send + 'static>(
    f: impl FnOnce() -> Result<R, ApiError> + Send + 'static,
) -> Response {
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(r)) => ok_response(r).into_response(),
        Ok(Err(e)) => err_response::<R>(e).into_response(),
        Err(e) => err_response::<R>(ApiError::internal(format!("service task failed: {e}")))
            .into_response(),
    }
}

#[derive(serde::Serialize)]
struct ApiEnvelope<T: serde::Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<ApiError>,
}

fn ok_response<T: serde::Serialize>(data: T) -> Json<ApiEnvelope<T>> {
    Json(ApiEnvelope {
        success: true,
        data: Some(data),
        error: None,
    })
}

fn err_response<T: serde::Serialize>(e: ApiError) -> (StatusCode, Json<ApiEnvelope<T>>) {
    let code = match e.code.as_str() {
        "NOT_FOUND" => StatusCode::NOT_FOUND,
        "INVALID_PARAMS" => StatusCode::BAD_REQUEST,
        "ALREADY_EXISTS" => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        code,
        Json(ApiEnvelope {
            success: false,
            data: None,
            error: Some(e),
        }),
    )
}

// ── Handler functions ──

async fn handle_init(State(state): State<Arc<AppState>>, Json(req): Json<InitRequest>) -> Response {
    run_blocking(move || state.service.init(req)).await
}

async fn handle_status(State(state): State<Arc<AppState>>) -> Response {
    run_blocking(move || state.service.status()).await
}

async fn handle_edit(State(state): State<Arc<AppState>>, Json(req): Json<EditRequest>) -> Response {
    run_blocking(move || state.service.edit(req)).await
}

async fn handle_agent_edit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<EditRequest>,
) -> Response {
    let agent_req = AgentEditRequest {
        agent_id: id,
        file: req.file,
        content: req.content,
    };
    run_blocking(move || state.service.agent_edit(agent_req)).await
}

async fn handle_agent_submit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    let req = AgentSubmitRequest { agent_id: id };
    run_blocking(move || state.service.agent_submit(req)).await
}

async fn handle_approve(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Response {
    let req = ApproveRequest { agent_id };
    run_blocking(move || state.service.approve(req)).await
}

async fn handle_commit(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CommitRequest>,
) -> Response {
    run_blocking(move || state.service.commit(req)).await
}

#[derive(serde::Deserialize)]
struct LogQuery {
    count: Option<usize>,
}

#[derive(serde::Deserialize)]
struct ShowQuery {
    show_what: String,
    target_id: Option<String>,
}

async fn handle_log(State(state): State<Arc<AppState>>, Query(query): Query<LogQuery>) -> Response {
    let req = LogRequest { count: query.count };
    run_blocking(move || state.service.log(req)).await
}

async fn handle_branch_list(State(state): State<Arc<AppState>>) -> Response {
    run_blocking(move || state.service.branch_list()).await
}

async fn handle_branch_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BranchCreateRequest>,
) -> Response {
    run_blocking(move || state.service.branch_create(req)).await
}

async fn handle_branch_switch(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Response {
    let req = BranchSwitchRequest { name };
    run_blocking(move || state.service.branch_switch(req)).await
}

async fn handle_merge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MergeRequest>,
) -> Response {
    run_blocking(move || state.service.merge(req)).await
}

async fn handle_backup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BackupRequest>,
) -> Response {
    run_blocking(move || state.service.backup(req)).await
}

async fn handle_restore(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RestoreRequest>,
) -> Response {
    run_blocking(move || state.service.restore(req)).await
}

async fn handle_gc(State(state): State<Arc<AppState>>) -> Response {
    let req = GcRequest {};
    run_blocking(move || state.service.gc(req)).await
}

async fn handle_git_commit(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GitCommitRequest>,
) -> Response {
    run_blocking(move || state.service.git_commit(req)).await
}

async fn handle_clean(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CleanRequest>,
) -> Response {
    run_blocking(move || state.service.clean(req)).await
}

async fn handle_pull(State(state): State<Arc<AppState>>, Json(req): Json<PullRequest>) -> Response {
    run_blocking(move || state.service.pull(req)).await
}

async fn handle_show(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ShowQuery>,
) -> Response {
    let req = ShowRequest {
        show_what: query.show_what,
        target_id: query.target_id,
    };
    run_blocking(move || state.service.show(req)).await
}

async fn handle_list_pending_approvals(State(state): State<Arc<AppState>>) -> Response {
    run_blocking(move || state.service.list_pending_approvals()).await
}

async fn handle_approve_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ApproveAgentRequest>,
) -> Response {
    run_blocking(move || state.service.approve_agent(req)).await
}

async fn handle_reject_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RejectAgentRequest>,
) -> Response {
    run_blocking(move || state.service.reject_agent(req)).await
}

async fn handle_merge_to_unified(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MergeToUnifiedRequest>,
) -> Response {
    run_blocking(move || state.service.merge_to_unified(req)).await
}

async fn handle_merge_to_staged(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MergeToStagedRequest>,
) -> Response {
    run_blocking(move || state.service.merge_to_staged(req)).await
}

async fn handle_compact(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompactRequest>,
) -> Response {
    run_blocking(move || state.service.compact(req)).await
}

async fn handle_checkpoint_restore(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckpointRestoreRequest>,
) -> Response {
    run_blocking(move || state.service.checkpoint_restore(req)).await
}

async fn handle_checkpoint_restore_by_time(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckpointRestoreByTimeRequest>,
) -> Response {
    run_blocking(move || state.service.checkpoint_restore_by_time(req)).await
}

async fn handle_checkpoint_diff(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckpointDiffRequest>,
) -> Response {
    run_blocking(move || state.service.checkpoint_diff(req)).await
}

async fn handle_checkpoint_rollback(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckpointRollbackRequest>,
) -> Response {
    run_blocking(move || state.service.checkpoint_rollback(req)).await
}
