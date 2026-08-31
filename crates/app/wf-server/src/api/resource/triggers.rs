//! Trigger entity surface: trigger CRUD / stats / enable / disable and
//! trigger execution records. Handlers are thin transport adapters over the
//! `wf-api::entity` trigger surfaces.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use wf_storage::adapter::trigger::TriggerListOptions;
use wf_storage::adapter::trigger_execution::TriggerExecutionListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{ExecutionIdPath, IdPath, ListQuery, NamePath};
use crate::router::ApiState;

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── triggers ──
        .route(
            "/triggers",
            get(handle_list_triggers).post(handle_save_trigger),
        )
        .route("/triggers/stats", get(handle_trigger_stats))
        .route("/triggers/search", get(handle_search_triggers))
        .route(
            "/triggers/{id}",
            get(handle_get_trigger).delete(handle_delete_trigger),
        )
        .route("/triggers/{id}/enable", post(handle_enable_trigger))
        .route("/triggers/{id}/disable", post(handle_disable_trigger))
        .route("/triggers/{id}/enabled", get(handle_trigger_enabled))
        // ── trigger executions ──
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
}

// ── triggers ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListTriggersQuery {
    #[serde(flatten)]
    page: ListQuery,
    event: Option<String>,
    enabled: Option<bool>,
}

async fn handle_list_triggers(
    State(state): State<ApiState>,
    Query(query): Query<ListTriggersQuery>,
) -> impl IntoResponse {
    let options = TriggerListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        event_filter: query.event,
        enabled_filter: query.enabled,
    };
    match wf_api::entity::trigger::list_triggers(&state.ctx.storage, Some(options)).await {
        Ok(triggers) => ok(triggers).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_save_trigger(
    State(state): State<ApiState>,
    Json(trigger): Json<wf_types::TriggerStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::save_trigger(&state.ctx.storage, &trigger).await {
        Ok(()) => ok(trigger.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_trigger(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::get_trigger(&state.ctx.storage, &path.id).await {
        Ok(trigger) => ok(trigger).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_trigger(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::delete_trigger(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::trigger::trigger_statistics(&state.ctx.storage).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_search_triggers(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::search_triggers(&state.ctx.storage, &query.q).await {
        Ok(triggers) => ok(triggers).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_enable_trigger(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::enable_trigger(&state.ctx.storage, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_disable_trigger(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::disable_trigger(&state.ctx.storage, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

/// Query the persisted enabled state of a trigger.
async fn handle_trigger_enabled(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::is_trigger_enabled(&state.ctx.storage, &path.id).await {
        Ok(enabled) => ok(enabled).into_response(),
        Err(e) => error_response(e),
    }
}

// ── trigger executions ────────────────────────────────────────────

#[derive(Deserialize)]
struct ListTriggerExecutionsQuery {
    #[serde(flatten)]
    page: ListQuery,
    trigger_name: Option<String>,
    execution_id: Option<String>,
    workflow_id: Option<String>,
    success: Option<bool>,
}

async fn handle_list_trigger_executions(
    State(state): State<ApiState>,
    Query(query): Query<ListTriggerExecutionsQuery>,
) -> impl IntoResponse {
    let options = TriggerExecutionListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        trigger_name_filter: query.trigger_name,
        execution_id_filter: query.execution_id,
        workflow_id_filter: query.workflow_id,
        success_filter: query.success,
    };
    match wf_api::entity::trigger_execution::list_trigger_executions(
        &state.ctx.storage,
        Some(options),
    )
    .await
    {
        Ok(executions) => ok(executions).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_save_trigger_execution(
    State(state): State<ApiState>,
    Json(execution): Json<wf_types::TriggerExecutionStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::entity::trigger_execution::save_trigger_execution(&state.ctx.storage, &execution)
        .await
    {
        Ok(()) => ok(execution.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_trigger_execution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger_execution::get_trigger_execution(&state.ctx.storage, &path.id)
        .await
    {
        Ok(execution) => ok(execution).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_trigger_execution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger_execution::delete_trigger_execution(&state.ctx.storage, &path.id)
        .await
    {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_execution_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::trigger_execution::get_trigger_execution_stats(&state.ctx.storage).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_executions_by_trigger(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger_execution::list_by_trigger_name(&state.ctx.storage, &path.name)
        .await
    {
        Ok(executions) => ok(executions).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_executions_by_execution(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger_execution::list_by_execution(
        &state.ctx.storage,
        &path.execution_id,
    )
    .await
    {
        Ok(executions) => ok(executions).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct CleanupTriggerExecutionsBody {
    older_than: Option<i64>,
}

async fn handle_cleanup_trigger_executions(
    State(state): State<ApiState>,
    Json(body): Json<CleanupTriggerExecutionsBody>,
) -> impl IntoResponse {
    let older_than = body.older_than.unwrap_or_else(wf_common::now);
    match wf_api::entity::trigger_execution::cleanup_old_trigger_executions(
        &state.ctx.storage,
        older_than,
    )
    .await
    {
        Ok(removed) => ok(removed).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_executions_by_workflow(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger_execution::list_by_workflow(&state.ctx.storage, &path.id).await {
        Ok(executions) => ok(executions).into_response(),
        Err(e) => error_response(e),
    }
}
