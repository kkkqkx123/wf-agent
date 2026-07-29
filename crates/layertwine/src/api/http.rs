//! HTTP transport layer for Layertwine API
//!
//! Provides an axum-based REST/JSON server that wraps the ApiService trait.
//! Enabled with `feature = "http"`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
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

async fn handle_init(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InitRequest>,
) -> impl IntoResponse {
    match state.service.init(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<InitResponse>(e).into_response(),
    }
}

async fn handle_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.service.status() {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<StatusResponse>(e).into_response(),
    }
}

async fn handle_edit(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EditRequest>,
) -> impl IntoResponse {
    match state.service.edit(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<EditResponse>(e).into_response(),
    }
}

async fn handle_agent_edit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<EditRequest>,
) -> impl IntoResponse {
    let agent_req = AgentEditRequest {
        agent_id: id,
        file: req.file,
        content: req.content,
    };
    match state.service.agent_edit(agent_req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<EditResponse>(e).into_response(),
    }
}

async fn handle_agent_submit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let req = AgentSubmitRequest { agent_id: id };
    match state.service.agent_submit(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<SubmitResponse>(e).into_response(),
    }
}

async fn handle_approve(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let req = ApproveRequest { agent_id };
    match state.service.approve(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<ApproveResponse>(e).into_response(),
    }
}

async fn handle_commit(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CommitRequest>,
) -> impl IntoResponse {
    match state.service.commit(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<CommitResponse>(e).into_response(),
    }
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

async fn handle_log(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LogQuery>,
) -> impl IntoResponse {
    let req = LogRequest { count: query.count };
    match state.service.log(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<LogResponse>(e).into_response(),
    }
}

async fn handle_branch_list(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.service.branch_list() {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<BranchListResponse>(e).into_response(),
    }
}

async fn handle_branch_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BranchCreateRequest>,
) -> impl IntoResponse {
    match state.service.branch_create(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<BranchCreateResponse>(e).into_response(),
    }
}

async fn handle_branch_switch(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let req = BranchSwitchRequest { name };
    match state.service.branch_switch(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<BranchSwitchResponse>(e).into_response(),
    }
}

async fn handle_merge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MergeRequest>,
) -> impl IntoResponse {
    match state.service.merge(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<MergeResponse>(e).into_response(),
    }
}

async fn handle_backup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BackupRequest>,
) -> impl IntoResponse {
    match state.service.backup(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<BackupResponse>(e).into_response(),
    }
}

async fn handle_restore(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RestoreRequest>,
) -> impl IntoResponse {
    match state.service.restore(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<RestoreResponse>(e).into_response(),
    }
}

async fn handle_gc(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let req = GcRequest {};
    match state.service.gc(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<GcResponse>(e).into_response(),
    }
}

async fn handle_git_commit(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GitCommitRequest>,
) -> impl IntoResponse {
    match state.service.git_commit(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<GitCommitResponse>(e).into_response(),
    }
}

async fn handle_clean(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CleanRequest>,
) -> impl IntoResponse {
    match state.service.clean(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<CleanResponse>(e).into_response(),
    }
}

async fn handle_pull(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PullRequest>,
) -> impl IntoResponse {
    match state.service.pull(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<PullResponse>(e).into_response(),
    }
}

async fn handle_show(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ShowQuery>,
) -> impl IntoResponse {
    let req = ShowRequest {
        show_what: query.show_what,
        target_id: query.target_id,
    };
    match state.service.show(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<ShowResponse>(e).into_response(),
    }
}

async fn handle_list_pending_approvals(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.service.list_pending_approvals() {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<ListPendingApprovalsResponse>(e).into_response(),
    }
}

async fn handle_approve_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ApproveAgentRequest>,
) -> impl IntoResponse {
    match state.service.approve_agent(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<ApproveAgentResponse>(e).into_response(),
    }
}

async fn handle_reject_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RejectAgentRequest>,
) -> impl IntoResponse {
    match state.service.reject_agent(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<RejectAgentResponse>(e).into_response(),
    }
}

async fn handle_merge_to_unified(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MergeToUnifiedRequest>,
) -> impl IntoResponse {
    match state.service.merge_to_unified(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<MergeToUnifiedResponse>(e).into_response(),
    }
}

async fn handle_merge_to_staged(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MergeToStagedRequest>,
) -> impl IntoResponse {
    match state.service.merge_to_staged(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<MergeToStagedResponse>(e).into_response(),
    }
}

async fn handle_compact(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompactRequest>,
) -> impl IntoResponse {
    match state.service.compact(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<CompactResponse>(e).into_response(),
    }
}

async fn handle_checkpoint_restore(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckpointRestoreRequest>,
) -> impl IntoResponse {
    match state.service.checkpoint_restore(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<CheckpointRestoreResponse>(e).into_response(),
    }
}

async fn handle_checkpoint_restore_by_time(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckpointRestoreByTimeRequest>,
) -> impl IntoResponse {
    match state.service.checkpoint_restore_by_time(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<CheckpointRestoreResponse>(e).into_response(),
    }
}

async fn handle_checkpoint_diff(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckpointDiffRequest>,
) -> impl IntoResponse {
    match state.service.checkpoint_diff(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<CheckpointDiffResponse>(e).into_response(),
    }
}

async fn handle_checkpoint_rollback(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckpointRollbackRequest>,
) -> impl IntoResponse {
    match state.service.checkpoint_rollback(req) {
        Ok(r) => ok_response(r).into_response(),
        Err(e) => err_response::<CheckpointRollbackResponse>(e).into_response(),
    }
}


