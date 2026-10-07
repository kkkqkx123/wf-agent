//! Agent loop CRUD and status handlers (transport adapters over
//! `wf-api::agent`).

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::envelope::{err, error_response, ok};
use crate::extract::IdPath;
use crate::paged::{fetch_size, ok_page, resolve_page_fields};
use crate::router::ApiState;

use super::inspect::parse_execution_status;

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListLoopsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    status: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops",
    tag = "agent",
    params(ListLoopsQuery),
    responses(
        (status = 200, description = "List of agent loops", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 400, description = "Invalid query parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_loops(
    State(state): State<ApiState>,
    Query(query): Query<ListLoopsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    let options = wf_api::AgentLoopListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        status_filter: query.status,
    };
    match wf_api::agent::list_agent_loops(&state.ctx.storage, Some(options)).await {
        Ok(loops) => ok_page(loops, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops",
    tag = "agent",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Agent loop created", body = crate::envelope::ApiEnvelope<String>),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Agent loop already exists", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_loop(
    State(state): State<ApiState>,
    Json(loop_def): Json<wf_api::AgentLoopStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::agent::save_agent_loop(&state.ctx.storage, &loop_def).await {
        Ok(()) => ok(loop_def.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Agent loop found", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::get_agent_loop(&state.ctx.storage, &path.id).await {
        Ok(loop_def) => ok(loop_def).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/agent-loops/{id}",
    tag = "agent",
    params(IdPath),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Agent loop updated", body = crate::envelope::ApiEnvelope<String>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut loop_def): Json<wf_api::AgentLoopStorageMetadata>,
) -> impl IntoResponse {
    loop_def.id = wf_api::Id::from(path.id.clone());
    match wf_api::agent::save_agent_loop(&state.ctx.storage, &loop_def).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/agent-loops/{id}",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Agent loop deleted", body = crate::envelope::ApiEnvelope<bool>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::delete_agent_loop(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct UpdateLoopStatusBody {
    /// New status value (e.g., "running", "paused", "completed", "failed")
    status: String,
}

#[utoipa::path(
    patch,
    path = "/api/v1/agent-loops/{id}/status",
    tag = "agent",
    params(IdPath),
    request_body = UpdateLoopStatusBody,
    responses(
        (status = 200, description = "Status updated", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid status", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_loop_status(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<UpdateLoopStatusBody>,
) -> impl IntoResponse {
    match wf_api::agent::update_agent_loop_status(&state.ctx.storage, &path.id, &body.status).await
    {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/status",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Current loop status", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_status(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::status(&state.ctx, &path.id).await {
        Ok(status) => ok(status).into_response(),
        Err(e) => error_response(e),
    }
}

/// Live status-machine transition: when the loop is running in memory the
/// coordinator entity pauses / resumes / stops directly; otherwise the
/// persisted metadata status is rewritten (fallback of `wf-api`).
#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/status/transition",
    tag = "agent",
    params(IdPath),
    request_body = UpdateLoopStatusBody,
    responses(
        (status = 200, description = "Status transition executed", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid status", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_status_transition(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<UpdateLoopStatusBody>,
) -> impl IntoResponse {
    let status = match parse_execution_status(&body.status) {
        Ok(status) => status,
        Err(message) => {
            return err(crate::envelope::ApiError::validation(message)).into_response()
        }
    };
    match wf_api::agent::agent_loop_registry::update_status(&state.ctx, &path.id, status).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

/// Remove all terminated (completed/failed/cancelled/stopped) live agent
/// loops from the in-memory registry.
#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/cleanup-completed",
    tag = "agent",
    responses(
        (status = 200, description = "Completed loops cleaned up", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_cleanup_completed(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::cleanup_completed(&state.ctx).await {
        Ok(count) => ok(count).into_response(),
        Err(e) => error_response(e),
    }
}
