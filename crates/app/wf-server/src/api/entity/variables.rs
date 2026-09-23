//! Variable entity surface: CRUD, batch / import, scopes, export, history
//! and stats. Handlers are thin transport adapters over the
//! `wf-api::entity::variable` surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_api::VariableListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{ExecutionIdPath, ListQuery, NamePath};
use crate::paged::{fetch_size, ok_page, resolve_page};
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
pub(crate) struct ListVariablesQuery {
    #[serde(flatten)]
    page: ListQuery,
    scope: Option<String>,
    execution_id: Option<String>,
}

#[utoipa::path(
    get,
    path = "/variables",
    tag = "entity",
    params(("limit" = Option<u64>, Query, description = "limit"), ("offset" = Option<u64>, Query, description = "offset"), ("scope" = Option<String>, Query, description = "scope"), ("execution_id" = Option<String>, Query, description = "execution_id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_variables(
    State(state): State<ApiState>,
    Query(query): Query<ListVariablesQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    let options = VariableListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        scope_filter: query.scope,
        execution_id_filter: query.execution_id,
    };
    match wf_api::entity::variable::list(&state.ctx, &options).await {
        Ok(variables) => ok_page(variables, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct VariableBody {
    name: String,
    value: Value,
    scope: Option<String>,
    execution_id: Option<String>,
    #[serde(rename = "define")]
    create_only: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/variables",
    tag = "entity",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_set_variable(
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
pub(crate) struct VariableQuery {
    scope: Option<String>,
    execution_id: Option<String>,
}

#[utoipa::path(
    get,
    path = "/variables/{name}",
    tag = "entity",
    params(("name" = String, Path, description = "name"), ("scope" = Option<String>, Query, description = "scope"), ("execution_id" = Option<String>, Query, description = "execution_id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_variable(
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

#[utoipa::path(
    delete,
    path = "/variables/{name}",
    tag = "entity",
    params(("name" = String, Path, description = "name"), ("scope" = Option<String>, Query, description = "scope"), ("execution_id" = Option<String>, Query, description = "execution_id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_variable(
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

#[utoipa::path(
    get,
    path = "/variables/stats",
    tag = "entity",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_variable_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::variable::variable_statistics(&state.ctx).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct VariableEntry {
    name: String,
    #[serde(default)]
    scope: String,
    value: Value,
}

#[derive(Deserialize)]
pub(crate) struct BatchSetVariablesBody {
    execution_id: String,
    entries: Vec<VariableEntry>,
}

/// Maximum entries per execution-scope variable batch; mirrors the loop
/// batch contract. The execution scope keeps fail-fast semantics.
const MAX_EXECUTION_BATCH_VARIABLES: usize = 100;

#[utoipa::path(
    post,
    path = "/variables/batch",
    tag = "entity",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_batch_set_variables(
    State(state): State<ApiState>,
    Json(body): Json<BatchSetVariablesBody>,
) -> impl IntoResponse {
    if body.entries.is_empty() {
        return error_response(wf_api::ApiError::Validation(
            "entries must not be empty".to_string(),
        ));
    }
    if body.entries.len() > MAX_EXECUTION_BATCH_VARIABLES {
        return error_response(wf_api::ApiError::Validation(format!(
            "at most {MAX_EXECUTION_BATCH_VARIABLES} entries per batch"
        )));
    }
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
pub(crate) struct ImportVariablesBody {
    execution_id: String,
    values: std::collections::BTreeMap<String, Value>,
}

#[utoipa::path(
    post,
    path = "/variables/import",
    tag = "entity",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import_variables(
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

/// Execution scopes stay a bare array: single-parent bounded vocabulary.
#[utoipa::path(
    get,
    path = "/variables/scopes/{executionId}",
    tag = "entity",
    params(("executionId" = String, Path, description = "executionId")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_variable_scopes(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::entity::variable::variable_scopes(&state.ctx, &path.execution_id).await {
        Ok(scopes) => ok(scopes).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct ScopePath {
    scope: String,
}

#[utoipa::path(
    get,
    path = "/variables/scope/{scope}",
    tag = "entity",
    params(("scope" = String, Path, description = "scope"), ("limit" = Option<u64>, Query, description = "limit"), ("offset" = Option<u64>, Query, description = "offset")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_variables_by_scope(
    State(state): State<ApiState>,
    Path(path): Path<ScopePath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::entity::variable::list_by_scope(&state.ctx, &path.scope).await {
        Ok(variables) => {
            let (limit, offset) = resolve_page(&query);
            let window = variables
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeVariablePath {
    execution_id: String,
    node_id: String,
}

/// Variables at one node stay a bare array: single-node bounded set.
#[utoipa::path(
    get,
    path = "/variables/by-node/{executionId}/{nodeId}",
    tag = "entity",
    params(("executionId" = String, Path, description = "executionId"), ("nodeId" = String, Path, description = "nodeId")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_variables_at_node(
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

#[utoipa::path(
    get,
    path = "/variables/export/{executionId}",
    tag = "entity",
    params(("executionId" = String, Path, description = "executionId"), ("download" = Option<bool>, Query, description = "download")),
    responses((status = 200, description = "Exported variables file download", body = String, content_type = "application/json"), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_variable_export(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
    Query(query): Query<VariableExportQuery>,
) -> impl IntoResponse {
    match wf_api::entity::variable::export(&state.ctx, &path.execution_id).await {
        Ok(export) => {
            if query.download.unwrap_or(false) {
                let payload = serde_json::to_string_pretty(&export).unwrap_or_default();
                crate::envelope::download(
                    &payload,
                    "application/json",
                    &format!("execution-{}-variables.json", path.execution_id),
                )
                .into_response()
            } else {
                ok(export).into_response()
            }
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct VariableExportQuery {
    download: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct VariableHistoryQuery {
    name: String,
    scope: Option<String>,
    execution_id: Option<String>,
    #[serde(flatten)]
    page: ListQuery,
}

#[utoipa::path(
    get,
    path = "/variables/history",
    tag = "entity",
    params(("name" = String, Query, description = "name"), ("scope" = Option<String>, Query, description = "scope"), ("execution_id" = Option<String>, Query, description = "execution_id"), ("limit" = Option<u64>, Query, description = "limit"), ("offset" = Option<u64>, Query, description = "offset")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_variable_history(
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
        Ok(history) => {
            let (limit, offset) = resolve_page(&query.page);
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
