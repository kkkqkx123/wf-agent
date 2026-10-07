//! Agent loop inspection handlers: summaries, statistics, iteration
//! history, timeline, variable history, context evolution and execution
//! path.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::envelope::{err, error_response, ok, ApiError};
use crate::extract::{CursorQuery, IdNamePath, IdPath, ListQuery};
use crate::paged::{
    fetch_size, ok_cursor_page, ok_page, resolve_cursor_page, resolve_page, resolve_page_fields,
};
use crate::router::ApiState;

pub(crate) fn parse_execution_status(status: &str) -> Result<wf_types::ExecutionStatus, String> {
    serde_json::from_value(serde_json::json!(status))
        .map_err(|_| format!("unknown execution status: {status}"))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct LoopSummariesQuery {
    /// Filter by execution status
    status: Option<String>,
    /// Filter by profile ID
    profile_id: Option<String>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    /// Sort by start time: `asc` or `desc`. Absent preserves registry order.
    order: Option<String>,
}

/// Live-first agent loop summaries (live registry merged with persisted
/// records), filtered by optional status / profile.
#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/summaries",
    tag = "agent",
    params(LoopSummariesQuery),
    responses(
        (status = 200, description = "List of agent loop summaries", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 400, description = "Invalid query parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_summaries(
    State(state): State<ApiState>,
    Query(query): Query<LoopSummariesQuery>,
) -> impl IntoResponse {
    let status = match query
        .status
        .as_deref()
        .map(parse_execution_status)
        .transpose()
    {
        Ok(status) => status,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    let filter = wf_api::AgentLoopFilter {
        ids: None,
        status,
        profile_id: query.profile_id,
        tags: None,
        created_at_range: None,
    };
    match wf_api::agent::agent_loop_registry::summaries(&state.ctx, Some(&filter)).await {
        Ok(mut summaries) => {
            match query.order.as_deref() {
                None => {}
                Some(raw) if raw.eq_ignore_ascii_case("desc") => {
                    summaries.sort_by_key(|s| std::cmp::Reverse(s.start_time.unwrap_or(i64::MIN)));
                }
                Some(raw) if raw.eq_ignore_ascii_case("asc") => {
                    summaries.sort_by_key(|s| s.start_time.unwrap_or(i64::MAX));
                }
                Some(raw) => {
                    return err(ApiError::validation(format!(
                        "unknown order: {raw} (expected asc or desc)"
                    )))
                    .into_response();
                }
            }
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
            let window = summaries
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// Aggregate agent loop statistics (total + per-status breakdown) across
/// live and persisted loops.
#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/stats",
    tag = "agent",
    responses(
        (status = 200, description = "Agent loop statistics", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_statistics(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::statistics(&state.ctx).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/summary",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Agent loop summary", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_summary(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::summary(&state.ctx, &path.id).await {
        Ok(summary) => ok(summary).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/iteration-history",
    tag = "agent",
    params(IdPath, ListQuery),
    responses(
        (status = 200, description = "Iteration history", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_iteration_history(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::iteration_history(&state.ctx, &path.id).await {
        Ok(history) => {
            let (limit, offset) = resolve_page(&query);
            let window = history
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/iteration-history/summary",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Iteration history summary", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_iteration_history_summary(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::iteration_history_summary(&state.ctx, &path.id).await
    {
        Ok(summary) => ok(summary).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/timeline",
    tag = "agent",
    params(IdPath, CursorQuery),
    responses(
        (status = 200, description = "Execution timeline", body = crate::envelope::ApiEnvelope<crate::paged::CursorPageView<serde_json::Value>>),
        (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_timeline(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<CursorQuery>,
) -> impl IntoResponse {
    let (limit, offset) = match resolve_cursor_page(query.limit, query.cursor.as_deref()) {
        Ok(page) => page,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    match wf_api::agent::agent_loop_registry::execution_timeline(&state.ctx, &path.id).await {
        Ok(timeline) => {
            let window = timeline
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_cursor_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/variable-history/{name}",
    operation_id = "get_agent_loops_id_variable_history_name",
    tag = "agent",
    params(IdNamePath, ListQuery),
    responses(
        (status = 200, description = "Variable history", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_variable_history(
    State(state): State<ApiState>,
    Path(path): Path<IdNamePath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::variable_history(&state.ctx, &path.id, &path.name)
        .await
    {
        Ok(history) => {
            let (limit, offset) = resolve_page(&query);
            let window = history
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/context-evolution",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Context evolution", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_context_evolution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::context_evolution(&state.ctx, &path.id).await {
        Ok(evolution) => ok(evolution).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/execution-path",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Execution path", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_execution_path(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::execution_path(&state.ctx, &path.id).await {
        Ok(path_view) => ok(path_view).into_response(),
        Err(e) => error_response(e),
    }
}
