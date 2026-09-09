//! File-checkpoint provenance endpoints: partition listing, change queries
//! by actor / path (with time-window filters), actor workspace
//! reconstruction and actor/staged diffs. Handlers are thin transport
//! adapters over `wf-api::workflow::file_provenance`.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/file-checkpoint/partitions", get(handle_list_partitions))
        .route(
            "/file-checkpoint/changes/actor/{id}",
            get(handle_list_changes_by_actor),
        )
        .route(
            "/file-checkpoint/changes/path/{id}",
            get(handle_list_changes_by_path),
        )
        .route(
            "/file-checkpoint/workspace/{id}",
            get(handle_get_actor_workspace),
        )
        .route(
            "/file-checkpoint/diff/actors/{a}/{b}",
            get(handle_diff_actors),
        )
        .route(
            "/file-checkpoint/diff/staged/{id}",
            get(handle_diff_against_staged),
        )
        .route("/file-checkpoint/gc", post(handle_run_gc))
        .route("/file-checkpoint/timeline/{id}", get(handle_file_timeline))
        .route(
            "/file-checkpoint/sessions",
            get(handle_list_sessions).post(handle_begin_session),
        )
        .route(
            "/file-checkpoint/sessions/{id}/rollback/{actor}",
            post(handle_rollback_session),
        )
        .route("/file-checkpoint/undo/{id}", post(handle_undo_edit))
        .route("/file-checkpoint/redo/{id}", post(handle_redo_edit))
        .route("/file-checkpoint/rename", post(handle_rename_file))
}

/// Actor / path query parameters: optional `path` substring filter and
/// inclusive `start` / `end` timestamp window (unix seconds).
#[derive(Debug, Default, Deserialize)]
struct ChangeQuery {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    start: Option<i64>,
    #[serde(default)]
    end: Option<i64>,
}

impl ChangeQuery {
    fn time_range(&self) -> Option<(i64, i64)> {
        match (self.start, self.end) {
            (None, None) => None,
            (start, end) => {
                let start_ms = start.unwrap_or(i64::MIN / 2).saturating_mul(1000);
                let end_ms = end.unwrap_or(i64::MAX / 2).saturating_mul(1000);
                Some((start_ms, end_ms))
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ActorPairPath {
    a: String,
    b: String,
}

async fn handle_list_partitions(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::list_partitions(&state.ctx) {
        Ok(partitions) => ok(partitions).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_list_changes_by_actor(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ChangeQuery>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::list_changes_by_actor(
        &state.ctx,
        &path.id,
        query.path.as_deref(),
        query.time_range(),
    ) {
        Ok(changes) => ok(changes).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_list_changes_by_path(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ChangeQuery>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::list_changes_by_path(
        &state.ctx,
        &path.id,
        query.time_range(),
    ) {
        Ok(changes) => ok(changes).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_get_actor_workspace(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::get_actor_workspace(&state.ctx, &path.id) {
        Ok(files) => ok(files).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_diff_actors(
    State(state): State<ApiState>,
    Path(path): Path<ActorPairPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::diff_actors(&state.ctx, &path.a, &path.b) {
        Ok(diffs) => ok(diffs).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_diff_against_staged(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::diff_against_staged(&state.ctx, &path.id) {
        Ok(diffs) => ok(diffs).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_file_timeline(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    // The route captures the file path as `id`; slashes arrive percent-encoded.
    match wf_api::checkpoint::provenance::file_timeline(&state.ctx, &path.id) {
        Ok(timeline) => ok(timeline).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Default, Deserialize)]
struct BeginSessionRequest {
    #[serde(default)]
    label: Option<String>,
}

async fn handle_begin_session(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<BeginSessionRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::begin_session(&state.ctx, body.label) {
        Ok(id) => ok(id).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_list_sessions(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::list_sessions(&state.ctx) {
        Ok(sessions) => ok(sessions).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize)]
struct SessionRollbackPath {
    id: String,
    actor: String,
}

async fn handle_rollback_session(
    State(state): State<ApiState>,
    Path(path): Path<SessionRollbackPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::rollback_session(&state.ctx, &path.actor, &path.id) {
        Ok(snapshot) => ok(snapshot).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_undo_edit(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::undo_edit(&state.ctx, &path.id) {
        Ok(snapshot) => ok(snapshot).into_response(),
        Err(err) => error_response(err),
    }
}

async fn handle_redo_edit(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::redo_edit(&state.ctx, &path.id) {
        Ok(snapshot) => ok(snapshot).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize)]
struct RenameFileRequest {
    actor: String,
    from_path: String,
    to_path: String,
    #[serde(default)]
    content: String,
}

async fn handle_rename_file(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<RenameFileRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::rename_file(
        &state.ctx,
        &body.actor,
        &body.from_path,
        &body.to_path,
        body.content.as_bytes(),
    ) {
        Ok(snapshot) => ok(snapshot).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Default, Deserialize)]
struct GcQuery {
    #[serde(default)]
    keep_recent_heads: usize,
}

async fn handle_run_gc(
    State(state): State<ApiState>,
    Query(query): Query<GcQuery>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::run_gc(&state.ctx, query.keep_recent_heads) {
        Ok(stats) => ok(stats).into_response(),
        Err(err) => error_response(err),
    }
}
