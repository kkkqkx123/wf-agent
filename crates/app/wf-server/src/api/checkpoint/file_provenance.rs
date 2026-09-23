//! File-checkpoint provenance endpoints: partition listing, paged change
//! queries, actor workspace reconstruction and actor/staged diffs. Handlers
//! are thin transport adapters over `wf-api::checkpoint::provenance`.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, ListQuery};
use crate::paged::{fetch_size, ok_page, resolve_page};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/file-checkpoint/partitions", get(handle_list_partitions))
        .route("/file-checkpoint/changes", get(handle_list_changes_paged))
        .route("/file-checkpoint/content", get(handle_read_content))
        .route("/file-checkpoint/tree/{id}", get(handle_list_tree))
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

#[utoipa::path(
    get,
    path = "/file-checkpoint/partitions",
    tag = "checkpoint",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_partitions(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::list_partitions(&state.ctx) {
        Ok(partitions) => ok(partitions).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    get,
    path = "/file-checkpoint/workspace/{id}",
    tag = "checkpoint",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_actor_workspace(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::get_actor_workspace(&state.ctx, &path.id) {
        Ok(files) => ok(files).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActorPairPath {
    a: String,
    b: String,
}

#[utoipa::path(
    get,
    path = "/file-checkpoint/diff/actors/{a}/{b}",
    tag = "checkpoint",
    params(("a" = String, Path, description = "a"), ("b" = String, Path, description = "b")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_diff_actors(
    State(state): State<ApiState>,
    Path(path): Path<ActorPairPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::diff_actors(&state.ctx, &path.a, &path.b) {
        Ok(diffs) => ok(diffs).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    get,
    path = "/file-checkpoint/diff/staged/{id}",
    tag = "checkpoint",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_diff_against_staged(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::diff_against_staged(&state.ctx, &path.id) {
        Ok(diffs) => ok(diffs).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    get,
    path = "/file-checkpoint/timeline/{id}",
    tag = "checkpoint",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_file_timeline(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    // The route captures the file path as `id`; slashes arrive percent-encoded.
    match wf_api::checkpoint::provenance::file_timeline_capped(&state.ctx, &path.id) {
        Ok(timeline) => ok(timeline).into_response(),
        Err(err) => error_response(err),
    }
}

/// Read-only file content: workspace current state with sandbox checks.
/// Snapshot-versioned reads are timeline-anchored (see `file_timeline` for
/// snapshot ids + hashes). Direct workspace mutations (rename, sessions,
/// undo/redo) are explicit file operations; the approval channel covers
/// human-in-the-loop tool approvals.
#[derive(Debug, Deserialize)]
pub(crate) struct ContentQuery {
    actor: String,
    path: String,
}

#[utoipa::path(
    get,
    path = "/file-checkpoint/content",
    tag = "checkpoint",
    params(("actor" = String, Query, description = "actor"), ("path" = String, Query, description = "path")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_read_content(
    State(state): State<ApiState>,
    Query(query): Query<ContentQuery>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::read_file_content(&state.ctx, &query.actor, &query.path) {
        Ok(view) => ok(view).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct TreeQuery {
    prefix: Option<String>,
}

/// Capped directory tree view with an explicit truncation flag.
#[derive(Debug, Serialize)]
pub(crate) struct FileTreeView {
    entries: Vec<wf_api::checkpoint::provenance::FileTreeEntry>,
    truncated: bool,
    total: usize,
}

#[utoipa::path(
    get,
    path = "/file-checkpoint/tree/{id}",
    tag = "checkpoint",
    params(("id" = String, Path, description = "id"), ("prefix" = Option<String>, Query, description = "prefix")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_tree(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<TreeQuery>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::list_tree_capped(
        &state.ctx,
        &path.id,
        query.prefix.as_deref(),
    ) {
        Ok(view) => ok(FileTreeView {
            entries: view.entries,
            truncated: view.truncated,
            total: view.total,
        })
        .into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PagedChangesQuery {
    actor: Option<String>,
    path: Option<String>,
    #[serde(default)]
    start: Option<i64>,
    #[serde(default)]
    end: Option<i64>,
    #[serde(flatten)]
    page: ListQuery,
}

#[utoipa::path(
    get,
    path = "/file-checkpoint/changes",
    tag = "checkpoint",
    params(("actor" = Option<String>, Query, description = "actor"), ("path" = Option<String>, Query, description = "path"), ("start" = Option<i64>, Query, description = "start"), ("end" = Option<i64>, Query, description = "end"), ("limit" = Option<u64>, Query, description = "limit"), ("offset" = Option<u64>, Query, description = "offset")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_changes_paged(
    State(state): State<ApiState>,
    Query(query): Query<PagedChangesQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    let time_range = match (query.start, query.end) {
        (None, None) => None,
        (start, end) => {
            let start_ms = start.unwrap_or(i64::MIN / 2).saturating_mul(1000);
            let end_ms = end.unwrap_or(i64::MAX / 2).saturating_mul(1000);
            Some((start_ms, end_ms))
        }
    };
    let changes = if let Some(actor) = query.actor.as_deref() {
        wf_api::checkpoint::provenance::list_changes_by_actor(
            &state.ctx,
            actor,
            query.path.as_deref(),
            time_range,
        )
    } else if let Some(path) = query.path.as_deref() {
        wf_api::checkpoint::provenance::list_changes_by_path(&state.ctx, path, time_range)
    } else {
        return error_response(wf_api::ApiError::Validation(
            "one of actor or path is required".to_string(),
        ));
    };
    match changes {
        Ok(all) => {
            let window = all
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct BeginSessionRequest {
    #[serde(default)]
    label: Option<String>,
}

#[utoipa::path(
    post,
    path = "/file-checkpoint/sessions",
    tag = "checkpoint",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_begin_session(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<BeginSessionRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::begin_edit_group(&state.ctx, body.label) {
        Ok(id) => ok(id).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    get,
    path = "/file-checkpoint/sessions",
    tag = "checkpoint",
    params(("limit" = Option<u64>, Query, description = "limit"), ("offset" = Option<u64>, Query, description = "offset")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_sessions(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::list_sessions(&state.ctx) {
        Ok(sessions) => {
            let (limit, offset) = resolve_page(&query);
            let window = sessions
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct SessionRollbackPath {
    id: String,
    actor: String,
}

#[utoipa::path(
    post,
    path = "/file-checkpoint/sessions/{id}/rollback/{actor}",
    tag = "checkpoint",
    params(("id" = String, Path, description = "id"), ("actor" = String, Path, description = "actor")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rollback_session(
    State(state): State<ApiState>,
    Path(path): Path<SessionRollbackPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::rollback_session(&state.ctx, &path.actor, &path.id) {
        Ok(snapshot) => ok(snapshot).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    post,
    path = "/file-checkpoint/undo/{id}",
    tag = "checkpoint",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_undo_edit(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::undo_edit(&state.ctx, &path.id) {
        Ok(snapshot) => ok(snapshot).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    post,
    path = "/file-checkpoint/redo/{id}",
    tag = "checkpoint",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_redo_edit(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::redo_edit(&state.ctx, &path.id) {
        Ok(snapshot) => ok(snapshot).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct RenameFileRequest {
    actor: String,
    from_path: String,
    to_path: String,
    #[serde(default)]
    content: String,
}

#[utoipa::path(
    post,
    path = "/file-checkpoint/rename",
    tag = "checkpoint",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rename_file(
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
pub(crate) struct GcQuery {
    #[serde(default)]
    keep_recent_heads: usize,
}

#[utoipa::path(
    post,
    path = "/file-checkpoint/gc",
    tag = "checkpoint",
    params(("keep_recent_heads" = u64, Query, description = "keep_recent_heads")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_run_gc(
    State(state): State<ApiState>,
    Query(query): Query<GcQuery>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::run_gc(&state.ctx, query.keep_recent_heads) {
        Ok(stats) => ok(stats).into_response(),
        Err(err) => error_response(err),
    }
}
