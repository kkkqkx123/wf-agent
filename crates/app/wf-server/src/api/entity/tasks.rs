//! Task entity surface: CRUD, stats, cleanup and cancel. Handlers are thin
//! transport adapters over the `wf-api::entity::task` surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use wf_api::TaskListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{ExecutionIdPath, IdPath, ListQuery};
use crate::paged::{fetch_size, ok_page, resolve_page, resolve_page_fields};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── tasks ──
        .route("/tasks", get(handle_list_tasks).post(handle_save_task))
        .route("/tasks/stats", get(handle_task_stats))
        .route(
            "/tasks/by-execution/{executionId}",
            get(handle_tasks_by_execution),
        )
        .route("/tasks/cleanup", post(handle_cleanup_tasks))
        .route(
            "/tasks/{id}",
            get(handle_get_task).delete(handle_delete_task),
        )
        .route("/tasks/{id}/cancel", post(handle_cancel_task))
}

// ── tasks ─────────────────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListTasksQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    status: Option<String>,
    task_type: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/tasks",
    tag = "entity",
    params(ListTasksQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_tasks(
    State(state): State<ApiState>,
    Query(query): Query<ListTasksQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    let options = TaskListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        status_filter: query.status,
        task_type_filter: query.task_type,
    };
    match wf_api::entity::task::list_tasks(&state.ctx.storage, Some(options)).await {
        Ok(tasks) => ok_page(tasks, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/tasks",
    tag = "entity",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_task(
    State(state): State<ApiState>,
    Json(task): Json<wf_api::TaskStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::entity::task::save_task(&state.ctx.storage, &task).await {
        Ok(()) => ok(task.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/tasks/{id}",
    tag = "entity",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_task(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::task::get_task(&state.ctx.storage, &path.id).await {
        Ok(task) => ok(task).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/tasks/{id}",
    tag = "entity",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_task(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::task::delete_task(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/tasks/stats",
    tag = "entity",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_task_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::task::get_task_stats(&state.ctx.storage).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/tasks/{id}/cancel",
    tag = "entity",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_cancel_task(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::task::cancel_task(&state.ctx.storage, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/tasks/by-execution/{executionId}",
    tag = "entity",
    params(ExecutionIdPath, ListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_tasks_by_execution(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::entity::task::get_by_execution_id(&state.ctx.storage, &path.execution_id).await {
        Ok(tasks) => {
            let (limit, offset) = resolve_page(&query);
            let window = tasks
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct CleanupTasksBody {
    older_than: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/v1/tasks/cleanup",
    tag = "entity",
    request_body = CleanupTasksBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_cleanup_tasks(
    State(state): State<ApiState>,
    Json(body): Json<CleanupTasksBody>,
) -> impl IntoResponse {
    let older_than = body.older_than.unwrap_or_else(wf_api::now);
    match wf_api::entity::task::cleanup_tasks(&state.ctx.storage, older_than).await {
        Ok(removed) => ok(removed).into_response(),
        Err(e) => error_response(e),
    }
}
