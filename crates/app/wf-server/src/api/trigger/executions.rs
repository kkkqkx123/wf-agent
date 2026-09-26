//! Trigger execution ledger surface: firing records of the event-driven
//! trigger listener. Handlers are thin transport adapters over the
//! `wf-api::trigger::execution` surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use wf_api::TriggerExecutionListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{ExecutionIdPath, IdPath, ListQuery, NamePath};
use crate::paged::{fetch_size, ok_page, resolve_page, resolve_page_fields};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── trigger executions (event-driven listener ledger) ──
        .route(
            "/trigger-executions",
            get(handle_list_trigger_executions).post(handle_save_trigger_execution),
        )
        .route(
            "/trigger-executions/cleanup",
            post(handle_cleanup_trigger_executions),
        )
        .route(
            "/trigger-executions/stats",
            get(handle_trigger_execution_stats),
        )
        .route(
            "/trigger-executions/by-execution/{executionId}",
            get(handle_trigger_executions_by_execution),
        )
        .route(
            "/trigger-executions/by-trigger/{name}",
            get(handle_trigger_executions_by_trigger),
        )
        .route(
            "/trigger-executions/by-workflow/{id}",
            get(handle_trigger_executions_by_workflow),
        )
        .route(
            "/trigger-executions/{id}",
            get(handle_get_trigger_execution).delete(handle_delete_trigger_execution),
        )
        .route("/triggers/history", get(handle_trigger_history))
}

// ── triggers ──────────────────────────────────────────────────────

// ── trigger executions ────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListTriggerExecutionsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    trigger_name: Option<String>,
    execution_id: Option<String>,
    workflow_id: Option<String>,
    /// Outcome filter: `completed` | `failed` | `abandoned`.
    outcome: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/trigger-executions",
    tag = "trigger",
    params(ListTriggerExecutionsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_trigger_executions(
    State(state): State<ApiState>,
    Query(query): Query<ListTriggerExecutionsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    let outcome_filter = match query.outcome.as_deref() {
        None => None,
        Some(raw) => match raw.parse::<wf_types::TriggerExecutionOutcome>() {
            Ok(outcome) => Some(outcome),
            Err(e) => {
                return crate::envelope::err(crate::envelope::ApiError::validation(e.to_string()))
                    .into_response()
            }
        },
    };
    let options = TriggerExecutionListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        trigger_name_filter: query.trigger_name,
        execution_id_filter: query.execution_id,
        workflow_id_filter: query.workflow_id,
        outcome_filter,
    };
    match wf_api::trigger::execution::list_trigger_executions(&state.ctx.storage, Some(options))
        .await
    {
        Ok(executions) => ok_page(executions, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/trigger-executions",
    tag = "trigger",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_trigger_execution(
    State(state): State<ApiState>,
    Json(execution): Json<wf_api::TriggerExecutionStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::trigger::execution::save_trigger_execution(&state.ctx.storage, &execution).await {
        Ok(()) => ok(execution.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/trigger-executions/{id}",
    tag = "trigger",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_trigger_execution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::trigger::execution::get_trigger_execution(&state.ctx.storage, &path.id).await {
        Ok(execution) => ok(execution).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/trigger-executions/{id}",
    tag = "trigger",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_trigger_execution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::trigger::execution::delete_trigger_execution(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/trigger-executions/stats",
    tag = "trigger",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_trigger_execution_stats(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    match wf_api::trigger::execution::get_trigger_execution_stats(&state.ctx.storage).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/trigger-executions/by-trigger/{name}",
    tag = "trigger",
    params(NamePath, ListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_trigger_executions_by_trigger(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::trigger::execution::list_by_trigger_name(&state.ctx.storage, &path.name).await {
        Ok(executions) => {
            let (limit, offset) = resolve_page(&query);
            let window = executions
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
    path = "/api/v1/trigger-executions/by-execution/{executionId}",
    tag = "trigger",
    params(ExecutionIdPath, ListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_trigger_executions_by_execution(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::trigger::execution::list_by_execution(&state.ctx.storage, &path.execution_id)
        .await
    {
        Ok(executions) => {
            let (limit, offset) = resolve_page(&query);
            let window = executions
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
pub(crate) struct CleanupTriggerExecutionsBody {
    older_than: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/v1/trigger-executions/cleanup",
    tag = "trigger",
    request_body = CleanupTriggerExecutionsBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_cleanup_trigger_executions(
    State(state): State<ApiState>,
    Json(body): Json<CleanupTriggerExecutionsBody>,
) -> impl IntoResponse {
    let older_than = body.older_than.unwrap_or_else(wf_api::now);
    match wf_api::trigger::execution::cleanup_old_trigger_executions(&state.ctx.storage, older_than)
        .await
    {
        Ok(removed) => ok(removed).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/trigger-executions/by-workflow/{id}",
    tag = "trigger",
    params(IdPath, ListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_trigger_executions_by_workflow(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::trigger::execution::list_by_workflow(&state.ctx.storage, &path.id).await {
        Ok(executions) => {
            let (limit, offset) = resolve_page(&query);
            let window = executions
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

// ── unified trigger history (ledger view, replaces legacy agent scope) ──

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct TriggerHistoryQuery {
    execution_id: String,
    trigger_name: Option<String>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/triggers/history",
    tag = "trigger",
    params(TriggerHistoryQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_trigger_history(
    State(state): State<ApiState>,
    Query(query): Query<TriggerHistoryQuery>,
) -> impl IntoResponse {
    match wf_api::trigger::execution::execution_history(
        &state.ctx.storage,
        &query.execution_id,
        query.trigger_name.as_deref(),
    )
    .await
    {
        Ok(history) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
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

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    #[tokio::test]
    async fn trigger_history_is_queryable() {
        let ctx = make_ctx();
        let response = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/triggers/history?execution_id=exec-1")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
