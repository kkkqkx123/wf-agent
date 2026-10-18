use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/git/meta", get(handle_git_meta))
        .route("/git/status", get(handle_git_status))
        .route("/git/files", get(handle_git_files))
        .route("/git/diff", get(handle_git_diff))
        .route("/git/diff/range", get(handle_git_diff_range))
        .route("/git/commits", get(handle_git_commits))
        .route("/git/commits/files", get(handle_git_commit_files))
        .route("/git/branches", get(handle_git_branches))
        .route("/git/remotes", get(handle_git_remotes))
        .route("/git/tags", get(handle_git_tags))
        .route("/git/stash", get(handle_git_stash))
        .route("/git/submodules", get(handle_git_submodules))
        .route("/git/worktrees", get(handle_git_worktrees))
        .route("/git/reflog", get(handle_git_reflog))
        .route("/git/fingerprint", get(handle_git_fingerprint))
        .route("/git/registry", get(handle_git_registry))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct DiffQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct DiffRangeQuery {
    from: String,
    to: String,
    path: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct CommitsQuery {
    limit: Option<u32>,
    offset: Option<u32>,
    branch: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct CommitFilesQuery {
    commit: String,
}

#[utoipa::path(get, path = "/api/v1/git/meta", operation_id = "get_git_meta", tag = "git", responses((status = 200, description = "Repository metadata", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "No repository", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal error", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_git_meta(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::meta(&state.ctx) {
        Ok(meta) => ok(meta).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/status", operation_id = "get_git_status", tag = "git", responses((status = 200, description = "Working tree status", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "No repository", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_git_status(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::status(&state.ctx) {
        Ok(view) => ok(view).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/files", operation_id = "get_git_files", tag = "git", responses((status = 200, description = "Three-state file list", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_files(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::file_entries(&state.ctx) {
        Ok(files) => ok(files).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/diff", operation_id = "get_git_diff", tag = "git", params(DiffQuery), responses((status = 200, description = "Unstaged diff by hunk", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_diff(
    State(state): State<ApiState>,
    Query(query): Query<DiffQuery>,
) -> impl IntoResponse {
    match wf_api::git::diff_unstaged(&state.ctx, query.path.as_deref()) {
        Ok(diff) => ok(diff).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/diff/range", operation_id = "get_git_diff_range", tag = "git", params(DiffRangeQuery), responses((status = 200, description = "Ref range diff by hunk", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_diff_range(
    State(state): State<ApiState>,
    Query(query): Query<DiffRangeQuery>,
) -> impl IntoResponse {
    match wf_api::git::diff_refs(&state.ctx, &query.from, &query.to, query.path.as_deref()) {
        Ok(diff) => ok(diff).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/commits", operation_id = "get_git_commits", tag = "git", params(CommitsQuery), responses((status = 200, description = "Commit list", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_commits(
    State(state): State<ApiState>,
    Query(query): Query<CommitsQuery>,
) -> impl IntoResponse {
    match wf_api::git::commits(
        &state.ctx,
        query.limit.unwrap_or(50),
        query.offset.unwrap_or(0),
        query.branch.as_deref(),
    ) {
        Ok(commits) => ok(commits).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/commits/files", operation_id = "get_git_commit_files", tag = "git", params(CommitFilesQuery), responses((status = 200, description = "Commit file list", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_commit_files(
    State(state): State<ApiState>,
    Query(query): Query<CommitFilesQuery>,
) -> impl IntoResponse {
    match wf_api::git::commit_files(&state.ctx, &query.commit) {
        Ok(files) => ok(files).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/branches", operation_id = "get_git_branches", tag = "git", responses((status = 200, description = "Local branches", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_branches(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::branches(&state.ctx) {
        Ok(branches) => ok(branches).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/remotes", operation_id = "get_git_remotes", tag = "git", responses((status = 200, description = "Remotes with branches", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_remotes(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::remotes(&state.ctx) {
        Ok(remotes) => ok(remotes).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/tags", operation_id = "get_git_tags", tag = "git", responses((status = 200, description = "Tags", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_tags(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::tags(&state.ctx) {
        Ok(tags) => ok(tags).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/stash", operation_id = "get_git_stash", tag = "git", responses((status = 200, description = "Stash list", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_stash(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::stash_list(&state.ctx) {
        Ok(entries) => ok(entries).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/submodules", operation_id = "get_git_submodules", tag = "git", responses((status = 200, description = "Submodules read-only", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_submodules(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::submodule_status(&state.ctx) {
        Ok(entries) => ok(entries).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/worktrees", operation_id = "get_git_worktrees", tag = "git", responses((status = 200, description = "Worktrees read-only", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_worktrees(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::worktrees(&state.ctx) {
        Ok(entries) => ok(entries).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/reflog", operation_id = "get_git_reflog", tag = "git", responses((status = 200, description = "Reflog entries", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_reflog(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::reflog(&state.ctx, 100) {
        Ok(entries) => ok(entries).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/fingerprint", operation_id = "get_git_fingerprint", tag = "git", responses((status = 200, description = "Repository state fingerprint", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_fingerprint(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::fingerprint(&state.ctx) {
        Ok(fp) => ok(fp).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/registry", operation_id = "get_git_registry", tag = "git", responses((status = 200, description = "Action registry snapshot", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_git_registry(State(state): State<ApiState>) -> impl IntoResponse {
    let _ = &state.ctx;
    ok(wf_api::git::registry_snapshot()).into_response()
}
