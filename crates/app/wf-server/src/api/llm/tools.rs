//! Tool registry surface: list / search / execute / validate / enable /
//! disable and the tool registry CRUD. Handlers are thin transport adapters
//! over the `wf-api::llm::tool` surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

use wf_api::ToolListOptions;
use wf_api::ToolStorageMetadata;

use crate::api::llm::scripts::DeleteForceQuery;
use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::paged::{fetch_size, ok_page, resolve_page_fields};
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

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListToolsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/tools",
    tag = "llm",
    params(ListToolsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_tools(
    State(state): State<ApiState>,
    Query(query): Query<ListToolsQuery>,
) -> impl IntoResponse {
    match wf_api::llm::tool::list(&state.ctx).await {
        Ok(tools) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
            let window = tools
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct SearchToolsQuery {
    q: String,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/tools/search",
    tag = "llm",
    params(SearchToolsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_search_tools(
    State(state): State<ApiState>,
    Query(query): Query<SearchToolsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    match wf_api::llm::tool::search_tools(&state.ctx, &query.q).await {
        Ok(tools) => {
            let window = tools
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
pub(crate) struct ExecuteToolBody {
    tool_id: String,
    parameters: Value,
    #[schema(value_type = Option<Object>)]
    options: Option<wf_api::ToolExecutionOptions>,
    execution_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/tools/execute",
    tag = "llm",
    request_body = ExecuteToolBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execute_tool(
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

#[derive(Deserialize, ToSchema)]
pub(crate) struct ValidateToolParamsBody {
    tool_id: String,
    parameters: Value,
}

#[utoipa::path(
    post,
    path = "/api/v1/tools/validate-params",
    tag = "llm",
    request_body = ValidateToolParamsBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_validate_tool_params(
    State(state): State<ApiState>,
    Json(body): Json<ValidateToolParamsBody>,
) -> impl IntoResponse {
    match wf_api::llm::tool::validate_parameters(&state.ctx, &body.tool_id, &body.parameters).await
    {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/tools/{id}",
    tag = "llm",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_tool(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::tool::get(&state.ctx, &path.id).await {
        Ok(tool) => ok(tool).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/tools/{id}/enable",
    tag = "llm",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_enable_tool(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::tool::enable(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/tools/{id}/disable",
    tag = "llm",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_disable_tool(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::tool::disable(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListToolRegistryQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    tool_type: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/tool-registry",
    tag = "llm",
    params(ListToolRegistryQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_tool_registry(
    State(state): State<ApiState>,
    Query(query): Query<ListToolRegistryQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    let options = ToolListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        tool_type_filter: query.tool_type,
    };
    match wf_api::llm::tool::list_tools(&state.ctx.storage, Some(options)).await {
        Ok(tools) => ok_page(tools, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/tool-registry",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_tool(
    State(state): State<ApiState>,
    Json(tool): Json<ToolStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::llm::tool::save_tool(&state.ctx.storage, &tool).await {
        Ok(()) => ok(tool.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/tool-registry/{id}",
    tag = "llm",
    params(IdPath, DeleteForceQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_tool(
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

#[utoipa::path(
    get,
    path = "/api/v1/tool-registry/stats",
    tag = "llm",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_tool_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::llm::tool::get_tool_stats(&state.ctx.storage).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}
