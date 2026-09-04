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
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
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
