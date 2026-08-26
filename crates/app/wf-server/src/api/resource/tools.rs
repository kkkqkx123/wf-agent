//! Tool registry surface: list / search / execute / validate / enable /
//! disable and the tool registry CRUD. Handlers are thin transport adapters
//! over the `wf-api::llm::tool` surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_storage::adapter::tool::ToolListOptions;
use wf_types::ToolStorageMetadata;

use crate::api::resource::scripts::DeleteForceQuery;
use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, ListQuery};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── tools ──
        .route("/tools", get(handle_list_tools))
        .route("/tools/search", get(handle_search_tools))
        .route("/tools/execute", post(handle_execute_tool))
        .route("/tools/validate-params", post(handle_validate_tool_params))
        .route("/tools/{id}", get(handle_get_tool))
        .route("/tools/{id}/enable", post(handle_enable_tool))
        .route("/tools/{id}/disable", post(handle_disable_tool))
        .route(
            "/tool-registry",
            get(handle_list_tool_registry).post(handle_save_tool),
        )
        .route("/tool-registry/{id}", delete(handle_delete_tool))
        .route("/tool-registry/stats", get(handle_tool_stats))
}

// ── tools ─────────────────────────────────────────────────────────

async fn handle_list_tools(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::llm::tool::list(&state.ctx).await {
        Ok(tools) => ok(tools).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SearchToolsQuery {
    q: String,
}

async fn handle_search_tools(
    State(state): State<ApiState>,
    Query(query): Query<SearchToolsQuery>,
) -> impl IntoResponse {
    match wf_api::llm::tool::search_tools(&state.ctx, &query.q).await {
        Ok(tools) => ok(tools).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ExecuteToolBody {
    tool_id: String,
    parameters: Value,
    options: Option<wf_types::tool::ToolExecutionOptions>,
    execution_id: Option<String>,
}

async fn handle_execute_tool(
    State(state): State<ApiState>,
    Json(body): Json<ExecuteToolBody>,
) -> impl IntoResponse {
    let execution_id = body.execution_id.as_deref().unwrap_or("adhoc");
    match wf_api::llm::tool::execute(
        &state.ctx,
        &body.tool_id,
        &body.parameters,
        body.options,
        execution_id,
    )
    .await
    {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ValidateToolParamsBody {
    tool_id: String,
    parameters: Value,
}

async fn handle_validate_tool_params(
    State(state): State<ApiState>,
    Json(body): Json<ValidateToolParamsBody>,
) -> impl IntoResponse {
    match wf_api::llm::tool::validate_parameters(&state.ctx, &body.tool_id, &body.parameters).await
    {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_tool(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::tool::get(&state.ctx, &path.id).await {
        Ok(tool) => ok(tool).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_enable_tool(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::tool::enable(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_disable_tool(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::tool::disable(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ListToolRegistryQuery {
    #[serde(flatten)]
    page: ListQuery,
    tool_type: Option<String>,
}

async fn handle_list_tool_registry(
    State(state): State<ApiState>,
    Query(query): Query<ListToolRegistryQuery>,
) -> impl IntoResponse {
    let options = ToolListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        tool_type_filter: query.tool_type,
    };
    match wf_api::llm::tool::list_tools(&state.ctx.storage, Some(options)).await {
        Ok(tools) => ok(tools).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_save_tool(
    State(state): State<ApiState>,
    Json(tool): Json<ToolStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::llm::tool::save_tool(&state.ctx.storage, &tool).await {
        Ok(()) => ok(tool.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_tool(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<DeleteForceQuery>,
) -> impl IntoResponse {
    match wf_api::infra::reference::delete_with_reference_check(
        &state.ctx,
        wf_api::ReferenceKind::Tool,
        &path.id,
        query.force.unwrap_or(false),
    )
    .await
    {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_tool_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::llm::tool::get_tool_stats(&state.ctx.storage).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}
