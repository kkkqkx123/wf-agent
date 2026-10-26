use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, patch, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/fs/list", get(handle_fs_list))
        .route("/fs/content", get(handle_fs_content).put(handle_fs_write))
        .route("/fs/stat", get(handle_fs_stat))
        .route(
            "/fs/entries",
            post(handle_fs_create).delete(handle_fs_delete),
        )
        .route("/fs/entries/move", patch(handle_fs_rename))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct FsPathQuery {
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct FsDeleteQuery {
    path: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct FsWriteBody {
    content: String,
    #[serde(default)]
    expected_hash: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct FsCreateBody {
    path: String,
    #[serde(default)]
    is_dir: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct FsRenameBody {
    from: String,
    to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct FsEntryView {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct FsFileView {
    pub path: String,
    pub content: String,
    pub size: u64,
    pub truncated: bool,
    pub is_binary: bool,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct FsStatView {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub is_binary: Option<bool>,
}

fn entry_view(entry: wf_api::fs::FsEntry) -> FsEntryView {
    FsEntryView {
        name: entry.name,
        path: entry.path,
        is_dir: entry.is_dir,
        size: entry.size,
        modified_ms: entry.modified_ms,
    }
}

fn file_view(content: wf_api::fs::FsFileContent) -> FsFileView {
    FsFileView {
        path: content.path,
        content: content.content,
        size: content.size,
        truncated: content.truncated,
        is_binary: content.is_binary,
        hash: content.hash,
    }
}

fn stat_view(stat: wf_api::fs::FsStat) -> FsStatView {
    FsStatView {
        path: stat.path,
        is_dir: stat.is_dir,
        size: stat.size,
        modified_ms: stat.modified_ms,
        is_binary: stat.is_binary,
    }
}

fn query_path(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

#[utoipa::path(get, path = "/api/v1/fs/list", operation_id = "get_fs_list", tag = "fs", params(FsPathQuery), responses((status = 200, description = "Directory listing", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "No workspace or no such path", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_list(
    State(state): State<ApiState>,
    Query(query): Query<FsPathQuery>,
) -> impl IntoResponse {
    match wf_api::fs::list_dir(&state.ctx, query_path(&query.path)) {
        Ok(entries) => ok(entries.into_iter().map(entry_view).collect::<Vec<_>>()).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/fs/content", operation_id = "get_fs_content", tag = "fs", params(FsPathQuery), responses((status = 200, description = "Text file content", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "No such file", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_content(
    State(state): State<ApiState>,
    Query(query): Query<FsPathQuery>,
) -> impl IntoResponse {
    match wf_api::fs::read_file(&state.ctx, query_path(&query.path)) {
        Ok(content) => ok(file_view(content)).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/fs/stat", operation_id = "get_fs_stat", tag = "fs", params(FsPathQuery), responses((status = 200, description = "Path metadata", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_stat(
    State(state): State<ApiState>,
    Query(query): Query<FsPathQuery>,
) -> impl IntoResponse {
    match wf_api::fs::stat_path(&state.ctx, query_path(&query.path)) {
        Ok(stat) => ok(stat_view(stat)).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(put, path = "/api/v1/fs/content", operation_id = "put_fs_content", tag = "fs", params(FsPathQuery), request_body = FsWriteBody, responses((status = 200, description = "File written", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 409, description = "Hash conflict", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_write(
    State(state): State<ApiState>,
    Query(query): Query<FsPathQuery>,
    axum::Json(body): axum::Json<FsWriteBody>,
) -> impl IntoResponse {
    match wf_api::fs::write_file(
        &state.ctx,
        query_path(&query.path),
        &body.content,
        body.expected_hash.as_deref(),
    ) {
        Ok(content) => ok(file_view(content)).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(post, path = "/api/v1/fs/entries", operation_id = "post_fs_entries", tag = "fs", request_body = FsCreateBody, responses((status = 200, description = "Entry created", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_create(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<FsCreateBody>,
) -> impl IntoResponse {
    match wf_api::fs::create_entry(&state.ctx, &body.path, body.is_dir) {
        Ok(entry) => ok(entry_view(entry)).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(delete, path = "/api/v1/fs/entries", operation_id = "delete_fs_entries", tag = "fs", params(FsDeleteQuery), responses((status = 200, description = "Entry deleted", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_delete(
    State(state): State<ApiState>,
    Query(query): Query<FsDeleteQuery>,
) -> impl IntoResponse {
    match wf_api::fs::delete_entry(&state.ctx, &query.path) {
        Ok(()) => ok(serde_json::json!({"removed": true, "path": query.path})).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(patch, path = "/api/v1/fs/entries/move", operation_id = "patch_fs_entries_move", tag = "fs", request_body = FsRenameBody, responses((status = 200, description = "Entry renamed", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_rename(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<FsRenameBody>,
) -> impl IntoResponse {
    match wf_api::fs::rename_entry(&state.ctx, &body.from, &body.to) {
        Ok(entry) => ok(entry_view(entry)).into_response(),
        Err(err) => error_response(err.into()),
    }
}
