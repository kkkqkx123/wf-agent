//! File-checkpoint provenance endpoints: partition listing, paged change
//! queries, actor workspace reconstruction and actor/main diffs. Handlers
//! are thin transport adapters over `wf-api::checkpoint::provenance`.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, ListQuery};
use crate::paged::{fetch_size, ok_page, resolve_page, resolve_page_fields};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/file-checkpoint/checkpoints",
            post(handle_create_checkpoint),
        )
        .route("/file-checkpoint/restore", post(handle_restore_checkpoint))
        .route("/file-checkpoint/partitions", get(handle_list_partitions))
        .route(
            "/file-checkpoint/partitions/{id}/history",
            get(handle_partition_history),
        )
        .route("/file-checkpoint/conflicts", get(handle_list_conflicts))
        .route(
            "/file-checkpoint/conflicts/resolve",
            post(handle_resolve_conflicts),
        )
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
            "/file-checkpoint/diff/main/{id}",
            get(handle_diff_against_main),
        )
        .route("/file-checkpoint/gc", post(handle_run_gc))
        .route("/file-checkpoint/gc/stats", get(handle_store_stats))
        .route("/file-checkpoint/state-links", get(handle_state_file_links))
        .route("/file-checkpoint/index/rebuild", post(handle_rebuild_index))
        .route("/file-checkpoint/index/status", get(handle_index_status))
        .route("/file-checkpoint/submit", post(handle_submit_review))
        .route(
            "/file-checkpoint/merge-to-feature",
            post(handle_merge_to_feature),
        )
        .route("/file-checkpoint/merge-to-main", post(handle_merge_to_main))
        .route(
            "/file-checkpoint/rollback-feature",
            post(handle_rollback_feature),
        )
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
        .route(
            "/file-checkpoint/undo/preview",
            post(handle_preview_undo_last_input),
        )
        .route(
            "/file-checkpoint/undo/apply",
            post(handle_apply_undo_last_input),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateCheckpointRequest {
    /// Actor the file edits are attributed to.
    actor: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct RestoreCheckpointResponse {
    restored: usize,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RestoreCheckpointRequest {
    /// Execution or entity id owning the checkpoint.
    entity: String,
    /// File checkpoint (commit) id to restore from.
    checkpoint: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/checkpoints",
    tag = "checkpoint",
    request_body = CreateCheckpointRequest,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::FileCheckpointSummaryDoc>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_create_checkpoint(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<CreateCheckpointRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::create_workspace_checkpoint(&state.ctx, &body.actor) {
        Ok(summary) => ok(summary).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/restore",
    tag = "checkpoint",
    request_body = RestoreCheckpointRequest,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::file_provenance::RestoreCheckpointResponse>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_restore_checkpoint(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<RestoreCheckpointRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::restore_workspace(
        &state.ctx,
        &body.entity,
        &body.checkpoint,
    ) {
        Ok(restored) => ok(RestoreCheckpointResponse { restored }).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/partitions",
    tag = "checkpoint",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<Vec<crate::api::checkpoint::views::PartitionDoc>>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_partitions(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::list_partitions(&state.ctx) {
        Ok(partitions) => ok(partitions).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct PartitionHistoryPath {
    id: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/partitions/{id}/history",
    tag = "checkpoint",
    params(PartitionHistoryPath, ListQuery),
    responses((status = 200, description = "Commit history of one partition ref, newest first", body = crate::envelope::ApiEnvelope<crate::paged::PageView<crate::api::checkpoint::views::PartitionHistoryEntryDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_partition_history(
    State(state): State<ApiState>,
    Path(path): Path<PartitionHistoryPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query);
    // One extra row detects the next page; the reader already skipped
    // `offset`, so only the window cap applies here.
    match wf_api::checkpoint::provenance::partition_history(
        &state.ctx,
        &path.id,
        fetch_size(limit) as usize,
        offset as usize,
    ) {
        Ok(all) => {
            let window = all.into_iter().take(fetch_size(limit) as usize).collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/conflicts",
    tag = "checkpoint",
    responses((status = 200, description = "Files with unresolved merge commits (Wf-State conflict-unresolved)", body = crate::envelope::ApiEnvelope<Vec<crate::api::checkpoint::views::ConflictFileDoc>>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_conflicts(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::list_conflicts(&state.ctx) {
        Ok(conflicts) => ok(conflicts).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/workspace/{id}",
    tag = "checkpoint",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<Vec<crate::api::checkpoint::views::WorkspaceFileDoc>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct ActorPairPath {
    a: String,
    b: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/diff/actors/{a}/{b}",
    tag = "checkpoint",
    params(ActorPairPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<Vec<crate::api::checkpoint::views::FileDiffDoc>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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
    path = "/api/v1/file-checkpoint/diff/main/{id}",
    tag = "checkpoint",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<Vec<crate::api::checkpoint::views::FileDiffDoc>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_diff_against_main(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::diff_against_main(&state.ctx, &path.id) {
        Ok(diffs) => ok(diffs).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/timeline/{id}",
    tag = "checkpoint",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::FileTimelineDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ContentQuery {
    actor: String,
    path: String,
    /// Commit id from the file timeline (hex): when present the bytes are
    /// read at that commit without touching the workspace. Named `version`
    /// for history but always a commit id, never a snapshot label.
    /// Reads are capped at 1 MiB with `truncated: true`; binaries report
    /// metadata only (`content: None`, `is_binary: true`).
    version: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/content",
    tag = "checkpoint",
    params(ContentQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::FileContentDoc>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_read_content(
    State(state): State<ApiState>,
    Query(query): Query<ContentQuery>,
) -> impl IntoResponse {
    let result = match query.version.as_deref() {
        Some(version) => wf_api::checkpoint::provenance::read_file_content_at(
            &state.ctx,
            &query.actor,
            &query.path,
            version,
        ),
        None => {
            wf_api::checkpoint::provenance::read_file_content(&state.ctx, &query.actor, &query.path)
        }
    };
    match result {
        Ok(view) => ok(view).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
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
    path = "/api/v1/file-checkpoint/tree/{id}",
    tag = "checkpoint",
    params(IdPath, TreeQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::FileTreeDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct PagedChangesQuery {
    actor: Option<String>,
    path: Option<String>,
    #[serde(default)]
    start: Option<i64>,
    #[serde(default)]
    end: Option<i64>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/changes",
    tag = "checkpoint",
    params(PagedChangesQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<crate::api::checkpoint::views::DeltaSummaryDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_changes_paged(
    State(state): State<ApiState>,
    Query(query): Query<PagedChangesQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
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

#[derive(Debug, Default, Deserialize, ToSchema)]
pub(crate) struct BeginSessionRequest {
    #[serde(default)]
    label: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/sessions",
    tag = "checkpoint",
    request_body = BeginSessionRequest,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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
    path = "/api/v1/file-checkpoint/sessions",
    tag = "checkpoint",
    params(ListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<crate::api::checkpoint::views::EditGroupDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_sessions(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::list_edit_groups(&state.ctx) {
        Ok(groups) => {
            let (limit, offset) = resolve_page(&query);
            let window = groups
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct SessionRollbackPath {
    id: String,
    actor: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/sessions/{id}/rollback/{actor}",
    tag = "checkpoint",
    params(SessionRollbackPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rollback_session(
    State(state): State<ApiState>,
    Path(path): Path<SessionRollbackPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::rollback_edit_group(&state.ctx, &path.actor, &path.id) {
        Ok(snapshot) => ok(snapshot).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/undo/{id}",
    tag = "checkpoint",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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
    path = "/api/v1/file-checkpoint/redo/{id}",
    tag = "checkpoint",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RenameFileRequest {
    actor: String,
    from_path: String,
    to_path: String,
    #[serde(default)]
    content: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/rename",
    tag = "checkpoint",
    request_body = RenameFileRequest,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct GcQuery {
    #[serde(default)]
    keep_recent_heads: usize,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/gc",
    tag = "checkpoint",
    params(GcQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::GcStatsDoc>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/gc/stats",
    tag = "checkpoint",
    responses((status = 200, description = "Object-store statistics: commits, refs, loose objects, on-disk bytes and last GC time", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::StoreStatsDoc>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_store_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::store_stats(&state.ctx) {
        Ok(stats) => ok(stats).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct StateLinksQuery {
    /// Comma-separated state checkpoint ids.
    ids: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/state-links",
    tag = "checkpoint",
    params(StateLinksQuery),
    responses((status = 200, description = "State-to-file association health per checkpoint: linked / pending / missing", body = crate::envelope::ApiEnvelope<Vec<crate::api::checkpoint::views::StateFileLinkDoc>>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_state_file_links(
    State(state): State<ApiState>,
    Query(query): Query<StateLinksQuery>,
) -> impl IntoResponse {
    let ids: Vec<String> = query
        .ids
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    match wf_api::checkpoint::provenance::state_file_links(&state.ctx, &ids) {
        Ok(links) => ok(links).into_response(),
        Err(err) => error_response(err),
    }
}

/// Rebuild response: number of indexed commits.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct RebuildIndexResponse {
    indexed_commits: usize,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/index/rebuild",
    tag = "checkpoint",
    responses((status = 200, description = "Source index rebuilt from the commit graph", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::file_provenance::RebuildIndexResponse>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rebuild_index(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::rebuild_source_index(&state.ctx) {
        Ok(indexed_commits) => ok(RebuildIndexResponse { indexed_commits }).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/file-checkpoint/index/status",
    tag = "checkpoint",
    responses((status = 200, description = "Source index health: indexed rows versus reachable commits", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::IndexStatusDoc>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_index_status(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::index_status(&state.ctx) {
        Ok(status) => ok(status).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct SubmitReviewRequest {
    /// Actor the edit line is attributed to (required, no anonymous writes).
    actor: String,
    /// Optional session attribution recorded alongside the actor.
    #[serde(default)]
    session: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SubmitReviewResponse {
    review_commit: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/submit",
    tag = "checkpoint",
    request_body = SubmitReviewRequest,
    responses((status = 200, description = "Edit line copied to a review ref", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::file_provenance::SubmitReviewResponse>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_submit_review(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<SubmitReviewRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::submit_for_review(
        &state.ctx,
        &body.actor,
        body.session.as_deref(),
    ) {
        Ok(review_commit) => ok(SubmitReviewResponse { review_commit }).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct MergeToFeatureRequest {
    actor: String,
    feature: String,
    #[serde(default)]
    session: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/merge-to-feature",
    tag = "checkpoint",
    request_body = MergeToFeatureRequest,
    responses((status = 200, description = "Actor submission merged into the feature ref", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::GitMergeOutcomeDoc>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_merge_to_feature(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<MergeToFeatureRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::merge_to_feature(
        &state.ctx,
        &body.actor,
        &body.feature,
        body.session.as_deref(),
    ) {
        Ok(outcome) => ok(outcome).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct MergeToMainRequest {
    actor: String,
    features: Vec<String>,
    #[serde(default)]
    session: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/merge-to-main",
    tag = "checkpoint",
    request_body = MergeToMainRequest,
    responses((status = 200, description = "Features merged into main, one merge commit per feature", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::MergeCommitDoc>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_merge_to_main(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<MergeToMainRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::merge_to_main(
        &state.ctx,
        &body.actor,
        &body.features,
        body.session.as_deref(),
    ) {
        Ok(result) => ok(result).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RollbackFeatureRequest {
    actor: String,
    feature_head: String,
    #[serde(default)]
    session: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct RollbackFeatureResponse {
    revert_commit: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/rollback-feature",
    tag = "checkpoint",
    request_body = RollbackFeatureRequest,
    responses((status = 200, description = "Revert commit appended; other features untouched", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::file_provenance::RollbackFeatureResponse>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rollback_feature(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<RollbackFeatureRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::provenance::rollback_feature(
        &state.ctx,
        &body.actor,
        &body.feature_head,
        body.session.as_deref(),
    ) {
        Ok(revert_commit) => ok(RollbackFeatureResponse { revert_commit }).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ResolveConflictsRequest {
    actor: String,
    feature: String,
    /// Resolved file contents keyed by workspace-relative path (base64
    /// callers decode before sending; transport carries raw JSON strings).
    resolutions: std::collections::HashMap<String, String>,
    #[serde(default)]
    session: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ResolveConflictsResponse {
    remaining: usize,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/conflicts/resolve",
    tag = "checkpoint",
    request_body = ResolveConflictsRequest,
    responses((status = 200, description = "Conflicts resolved on the feature ref; remaining counts unresolved files", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::file_provenance::ResolveConflictsResponse>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_resolve_conflicts(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<ResolveConflictsRequest>,
) -> impl IntoResponse {
    let resolutions: Vec<(String, Vec<u8>)> = body
        .resolutions
        .into_iter()
        .map(|(path, content)| (path, content.into_bytes()))
        .collect();
    match wf_api::checkpoint::provenance::resolve_feature_conflicts(
        &state.ctx,
        &body.actor,
        &body.feature,
        resolutions,
        body.session.as_deref(),
    ) {
        Ok(remaining) => ok(ResolveConflictsResponse { remaining }).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct UndoLastInputRequest {
    #[serde(default)]
    pub execution_id: String,
    #[serde(default)]
    pub checkpoint_prefixes: Option<Vec<String>>,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/undo/preview",
    tag = "checkpoint",
    request_body = UndoLastInputRequest,
    responses((status = 200, description = "Undo preview for the last input boundary", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::views::UndoPreviewDoc>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_preview_undo_last_input(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<UndoLastInputRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::undo::preview_undo_last_input(
        &state.ctx,
        &body.execution_id,
        body.checkpoint_prefixes,
    )
    .await
    {
        Ok(preview) => ok(preview).into_response(),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ApplyUndoResponse {
    restored: usize,
}

#[utoipa::path(
    post,
    path = "/api/v1/file-checkpoint/undo/apply",
    tag = "checkpoint",
    request_body = UndoLastInputRequest,
    responses((status = 200, description = "Undo of the last input boundary applied", body = crate::envelope::ApiEnvelope<crate::api::checkpoint::file_provenance::ApplyUndoResponse>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_apply_undo_last_input(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<UndoLastInputRequest>,
) -> impl IntoResponse {
    match wf_api::checkpoint::undo::apply_undo_last_input(
        &state.ctx,
        &body.execution_id,
        body.checkpoint_prefixes,
    )
    .await
    {
        Ok(restored) => ok(ApplyUndoResponse { restored }).into_response(),
        Err(err) => error_response(err),
    }
}
