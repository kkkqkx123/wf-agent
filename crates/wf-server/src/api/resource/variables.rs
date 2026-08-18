//! Variable entity surface: CRUD, batch / import, scopes, export, history
//! and stats. Handlers are thin transport adapters over the
//! `wf-api::entity::variable` surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_storage::adapter::variable::VariableListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{ExecutionIdPath, ListQuery, NamePath};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── variables ──
        .route(
            "/variables",
            get(handle_list_variables).post(handle_set_variable),
        )
        .route("/variables/batch", post(handle_batch_set_variables))
        .route("/variables/import", post(handle_import_variables))
        .route(
            "/variables/scopes/{executionId}",
            get(handle_variable_scopes),
        )
        .route("/variables/scope/{scope}", get(handle_variables_by_scope))
        .route(
            "/variables/by-node/{executionId}/{nodeId}",
            get(handle_variables_at_node),
        )
        .route("/variables/stats", get(handle_variable_stats))
        .route("/variables/history", get(handle_variable_history))
        .route(
            "/variables/export/{executionId}",
            get(handle_variable_export),
        )
        .route(
            "/variables/{name}",
            get(handle_get_variable).delete(handle_delete_variable),
        )
}

// ── variables ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListVariablesQuery {
    #[serde(flatten)]
    page: ListQuery,
    scope: Option<String>,
    execution_id: Option<String>,
}

async fn handle_list_variables(
    State(state): State<ApiState>,
    Query(query): Query<ListVariablesQuery>,
) -> impl IntoResponse {
    let options = VariableListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        scope_filter: query.scope,
        execution_id_filter: query.execution_id,
    };
    match wf_api::entity::variable::list(&state.ctx, &options).await {
        Ok(variables) => ok(variables).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct VariableBody {
    name: String,
    value: Value,
    scope: Option<String>,
    execution_id: Option<String>,
    #[serde(rename = "define")]
    create_only: Option<bool>,
}

async fn handle_set_variable(
    State(state): State<ApiState>,
    Json(body): Json<VariableBody>,
) -> impl IntoResponse {
    let scope = body.scope.as_deref().unwrap_or("default");
    let result = if body.create_only.unwrap_or(false) {
        wf_api::entity::variable::define(
            &state.ctx,
            &body.name,
            scope,
            body.execution_id.as_deref(),
            body.value,
        )
        .await
    } else {
        wf_api::entity::variable::set(
            &state.ctx,
            &body.name,
            scope,
            body.execution_id.as_deref(),
            body.value,
        )
        .await
    };
    match result {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct VariableQuery {
    scope: Option<String>,
    execution_id: Option<String>,
}

async fn handle_get_variable(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
    Query(query): Query<VariableQuery>,
) -> impl IntoResponse {
    match wf_api::entity::variable::get(
        &state.ctx,
        &path.name,
        query.scope.as_deref().unwrap_or("default"),
        query.execution_id.as_deref(),
    )
    .await
    {
        Ok(variable) => ok(variable).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_variable(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
    Query(query): Query<VariableQuery>,
) -> impl IntoResponse {
    match wf_api::entity::variable::delete(
        &state.ctx,
        &path.name,
        query.scope.as_deref().unwrap_or("default"),
        query.execution_id.as_deref(),
    )
    .await
    {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_variable_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::variable::variable_statistics(&state.ctx).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct VariableEntry {
    name: String,
    #[serde(default)]
    scope: String,
    value: Value,
}

#[derive(Deserialize)]
struct BatchSetVariablesBody {
    execution_id: String,
    entries: Vec<VariableEntry>,
}

async fn handle_batch_set_variables(
    State(state): State<ApiState>,
    Json(body): Json<BatchSetVariablesBody>,
) -> impl IntoResponse {
    let entries: Vec<(String, String, Value)> = body
        .entries
        .into_iter()
        .map(|entry| (entry.name, entry.scope, entry.value))
        .collect();
    match wf_api::entity::variable::batch_set_variables(&state.ctx, &body.execution_id, &entries)
        .await
    {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ImportVariablesBody {
    execution_id: String,
    values: std::collections::BTreeMap<String, Value>,
}

async fn handle_import_variables(
    State(state): State<ApiState>,
    Json(body): Json<ImportVariablesBody>,
) -> impl IntoResponse {
    match wf_api::entity::variable::import_variables(&state.ctx, &body.execution_id, &body.values)
        .await
    {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_variable_scopes(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::entity::variable::variable_scopes(&state.ctx, &path.execution_id).await {
        Ok(scopes) => ok(scopes).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ScopePath {
    scope: String,
}

async fn handle_variables_by_scope(
    State(state): State<ApiState>,
    Path(path): Path<ScopePath>,
) -> impl IntoResponse {
    match wf_api::entity::variable::list_by_scope(&state.ctx, &path.scope).await {
        Ok(variables) => ok(variables).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NodeVariablePath {
    execution_id: String,
    node_id: String,
}

async fn handle_variables_at_node(
    State(state): State<ApiState>,
    Path(path): Path<NodeVariablePath>,
) -> impl IntoResponse {
    match wf_api::entity::variable::variables_at_node(&state.ctx, &path.execution_id, &path.node_id)
        .await
    {
        Ok(variables) => ok(variables).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_variable_export(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::entity::variable::export(&state.ctx, &path.execution_id).await {
        Ok(export) => ok(export).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct VariableHistoryQuery {
    name: String,
    scope: Option<String>,
    execution_id: Option<String>,
}

async fn handle_variable_history(
    State(state): State<ApiState>,
    Query(query): Query<VariableHistoryQuery>,
) -> impl IntoResponse {
    match wf_api::entity::variable::history(
        &state.ctx,
        &query.name,
        query.scope.as_deref().unwrap_or("default"),
        query.execution_id.as_deref(),
    )
    .await
    {
        Ok(history) => ok(history).into_response(),
        Err(e) => error_response(e),
    }
}
