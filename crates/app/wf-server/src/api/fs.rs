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
        .route("/fs/tree", get(handle_fs_tree))
        .route("/fs/search", get(handle_fs_search))
        .route("/fs/content", get(handle_fs_content).put(handle_fs_write))
        .route("/fs/stat", get(handle_fs_stat))
        .route("/fs/raw", get(handle_fs_raw))
        .route("/fs/upload", post(handle_fs_upload))
        .route(
            "/fs/entries",
            post(handle_fs_create).delete(handle_fs_delete),
        )
        .route("/fs/entries/move", patch(handle_fs_rename))
        .route("/fs/entries/copy", post(handle_fs_copy))
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

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct FsCopyBody {
    from: String,
    to: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct FsUploadBody {
    path: String,
    content_base64: String,
    #[serde(default)]
    overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct FsRawView {
    pub path: String,
    pub filename: String,
    pub mime: String,
    pub size: u64,
    pub content_base64: String,
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
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct FsSearchMatchView {
    pub path: String,
    pub line: usize,
    pub column: Option<usize>,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct FsSearchView {
    pub matches: Vec<FsSearchMatchView>,
    pub truncated: bool,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct FsSearchQuery {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    include: Option<String>,
    #[serde(default)]
    case_sensitive: Option<bool>,
    #[serde(default)]
    regex: Option<bool>,
    #[serde(default)]
    whole_word: Option<bool>,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct FsTreeEntryView {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct FsTreeView {
    pub entries: Vec<FsTreeEntryView>,
    pub truncated: bool,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct FsTreeQuery {
    #[serde(default)]
    path: Option<String>,
    depth: Option<u32>,
    limit: Option<usize>,
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
        hash: stat.hash,
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

#[utoipa::path(get, path = "/api/v1/fs/tree", operation_id = "get_fs_tree", tag = "fs", params(FsTreeQuery), responses((status = 200, description = "Recursive file index", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "No workspace or no such path", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_tree(
    State(state): State<ApiState>,
    Query(query): Query<FsTreeQuery>,
) -> impl IntoResponse {
    match wf_api::fs::list_tree(
        &state.ctx,
        query_path(&query.path),
        query.depth,
        query.limit,
    ) {
        Ok(tree) => ok(FsTreeView {
            entries: tree
                .entries
                .into_iter()
                .map(|e| FsTreeEntryView {
                    path: e.path,
                    is_dir: e.is_dir,
                    size: e.size,
                })
                .collect(),
            truncated: tree.truncated,
        })
        .into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/fs/search", operation_id = "get_fs_search", tag = "fs", params(FsSearchQuery), responses((status = 200, description = "Workspace content search", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "No workspace or no such path", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_search(
    State(state): State<ApiState>,
    Query(query): Query<FsSearchQuery>,
) -> impl IntoResponse {
    let needle = query.query.as_deref().unwrap_or("").trim();
    if needle.is_empty() {
        return ok(FsSearchView {
            matches: Vec::new(),
            truncated: false,
        })
        .into_response();
    }
    match wf_api::fs::search_files(
        &state.ctx,
        needle,
        query_path(&query.path),
        query.include.as_deref(),
        query.case_sensitive.unwrap_or(false),
        query.regex.unwrap_or(false),
        query.whole_word.unwrap_or(false),
        query.offset.unwrap_or(0),
        query.limit,
    ) {
        Ok(result) => ok(FsSearchView {
            matches: result
                .matches
                .into_iter()
                .map(|m| FsSearchMatchView {
                    path: m.path,
                    line: m.line,
                    column: m.column,
                    preview: m.preview,
                })
                .collect(),
            truncated: result.truncated,
        })
        .into_response(),
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

#[utoipa::path(post, path = "/api/v1/fs/entries/copy", operation_id = "post_fs_entries_copy", tag = "fs", request_body = FsCopyBody, responses((status = 200, description = "Entry copied", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_copy(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<FsCopyBody>,
) -> impl IntoResponse {
    match wf_api::fs::copy_entry(&state.ctx, &body.from, &body.to) {
        Ok(entry) => ok(entry_view(entry)).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/fs/raw", operation_id = "get_fs_raw", tag = "fs", params(FsPathQuery), responses((status = 200, description = "Raw file bytes as base64", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_raw(
    State(state): State<ApiState>,
    Query(query): Query<FsPathQuery>,
) -> impl IntoResponse {
    match wf_api::fs::read_raw(&state.ctx, query_path(&query.path)) {
        Ok(raw) => ok(FsRawView {
            path: raw.path,
            filename: raw.filename,
            mime: raw.mime,
            size: raw.size,
            content_base64: raw.content_base64,
        })
        .into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(post, path = "/api/v1/fs/upload", operation_id = "post_fs_upload", tag = "fs", request_body = FsUploadBody, responses((status = 200, description = "File uploaded", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_fs_upload(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<FsUploadBody>,
) -> impl IntoResponse {
    match wf_api::fs::upload_bytes(&state.ctx, &body.path, &body.content_base64, body.overwrite) {
        Ok(entry) => ok(entry_view(entry)).into_response(),
        Err(err) => error_response(err.into()),
    }
}
