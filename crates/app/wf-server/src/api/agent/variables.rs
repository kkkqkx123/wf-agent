//! Agent message and variable surfaces: per-loop messages / conversation /
//! variables. Handlers are thin transport adapters over the `wf-api::agent`
//! message and variable surfaces.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

use crate::api::web::batch::BatchItemResult;
use crate::envelope::{error_response, ok};
use crate::extract::{IdNamePath, IdPath, ListQuery};
use crate::paged::{fetch_size, ok_page, resolve_page, resolve_page_fields};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── agent messages ──
        .route("/agent-loops/{id}/messages", get(handle_recent_messages))
        .route(
            "/agent-loops/{id}/messages/search",
            get(handle_search_messages),
        )
        .route(
            "/agent-loops/{id}/messages/stats",
            get(handle_message_stats),
        )
        .route(
            "/agent-loops/{id}/messages/dedupe",
            post(handle_dedupe_messages),
        )
        .route("/agent-loops/{id}/conversation", get(handle_conversation))
        // ── agent variables ──
        .route("/agent-loops/{id}/variables", get(handle_list_variables))
        .route(
            "/agent-loops/{id}/variables/stats",
            get(handle_variable_stats),
        )
        .route(
            "/agent-loops/{id}/variables/export",
            get(handle_variable_export),
        )
        // Static `batch` is matched before dynamic `{name}` by the router;
        // keep this registration after the `{name}` route only with this
        // invariant in mind.
        .route(
            "/agent-loops/{id}/variables/{name}",
            get(handle_get_variable)
                .put(handle_set_variable)
                .delete(handle_delete_variable),
        )
        .route(
            "/agent-loops/{id}/variables/batch",
            post(handle_batch_set_loop_variables),
        )
}

// ── agent messages ────────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct RecentMessagesQuery {
    /// Optional count limit for recent messages
    count: Option<usize>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/messages",
    tag = "agent",
    params(IdPath, RecentMessagesQuery),
    responses(
        (status = 200, description = "Recent messages", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_recent_messages(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<RecentMessagesQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_message::recent(&state.ctx, &path.id, query.count).await {
        Ok(messages) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
            let window = messages
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// Delete duplicate messages of an agent loop from storage; returns the
/// number of removed records.
#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/messages/dedupe",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Number of duplicate messages removed", body = crate::envelope::ApiEnvelope<usize>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_dedupe_messages(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_message::dedupe_and_delete(&state.ctx, &path.id).await {
        Ok(removed) => ok(removed).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct SearchMessagesQuery {
    /// Search query string
    q: String,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/messages/search",
    tag = "agent",
    params(IdPath, SearchMessagesQuery),
    responses(
        (status = 200, description = "Search results", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_search_messages(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<SearchMessagesQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_message::search(&state.ctx, &path.id, &query.q).await {
        Ok(messages) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
            let window = messages
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
    path = "/api/v1/agent-loops/{id}/messages/stats",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Message statistics", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_message_stats(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_message::stats(&state.ctx, &path.id).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ConversationQuery {
    /// Maximum number of messages to return
    max_messages: Option<usize>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/conversation",
    tag = "agent",
    params(IdPath, ConversationQuery),
    responses(
        (status = 200, description = "Conversation history", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_conversation(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ConversationQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_message::conversation_history(
        &state.ctx,
        &path.id,
        query.max_messages,
    )
    .await
    {
        Ok(messages) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
            let window = messages
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

// ── agent variables ───────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/variables",
    tag = "agent",
    params(IdPath, ListQuery),
    responses(
        (status = 200, description = "List of variables", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_variables(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_variable::get_execution_variables(&state.ctx, &path.id).await {
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

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/variables/stats",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Variable statistics", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_variable_stats(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_variable::get_variable_statistics(&state.ctx, &path.id).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct VariableExportQuery {
    /// If true, return as downloadable attachment
    download: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/variables/export",
    tag = "agent",
    params(IdPath, VariableExportQuery),
    responses(
        (status = 200, description = "Exported variables file download", body = String, content_type = "application/json"),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_variable_export(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<VariableExportQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_variable::export_execution_variables(&state.ctx, &path.id).await {
        Ok(export) => {
            if query.download.unwrap_or(false) {
                crate::envelope::download(
                    &export,
                    "application/json",
                    &format!("loop-{}-variables.json", path.id),
                )
                .into_response()
            } else {
                ok(export).into_response()
            }
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/variables/{name}",
    tag = "agent",
    params(IdNamePath),
    responses(
        (status = 200, description = "Variable value", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_variable(
    State(state): State<ApiState>,
    Path(path): Path<IdNamePath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_variable::get_execution_variable(&state.ctx, &path.id, &path.name)
        .await
    {
        Ok(value) => ok(value).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetVariableBody {
    /// Variable value (any JSON)
    value: Value,
}

#[utoipa::path(
    put,
    path = "/api/v1/agent-loops/{id}/variables/{name}",
    tag = "agent",
    params(IdNamePath),
    request_body = SetVariableBody,
    responses(
        (status = 200, description = "Variable set", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_set_variable(
    State(state): State<ApiState>,
    Path(path): Path<IdNamePath>,
    Json(body): Json<SetVariableBody>,
) -> impl IntoResponse {
    match wf_api::agent::agent_variable::set_variable(&state.ctx, &path.id, &path.name, body.value)
        .await
    {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/agent-loops/{id}/variables/{name}",
    tag = "agent",
    params(IdNamePath),
    responses(
        (status = 200, description = "Variable deleted", body = crate::envelope::ApiEnvelope<bool>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_variable(
    State(state): State<ApiState>,
    Path(path): Path<IdNamePath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_variable::delete_variable(&state.ctx, &path.id, &path.name).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

/// Maximum entries per loop-variable batch; mirrors the web batch contract.
const MAX_LOOP_BATCH_VARIABLES: usize = 100;

#[derive(Deserialize, ToSchema)]
pub(crate) struct LoopVariableBatchEntry {
    /// Variable name
    name: String,
    /// Variable value (any JSON)
    value: Value,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct LoopVariableBatchBody {
    /// Variable entries (1-100)
    entries: Vec<LoopVariableBatchEntry>,
}

/// Batch write of loop-scoped variables with per-item reporting. Unlike the
/// execution-scope fail-fast batch, one bad entry never aborts the rest.
#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/variables/batch",
    tag = "agent",
    params(IdPath),
    request_body = LoopVariableBatchBody,
    responses(
        (status = 200, description = "Batch results", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_batch_set_loop_variables(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<LoopVariableBatchBody>,
) -> impl IntoResponse {
    if body.entries.is_empty() {
        return error_response(wf_api::ApiError::Validation(
            "entries must not be empty".to_string(),
        ));
    }
    if body.entries.len() > MAX_LOOP_BATCH_VARIABLES {
        return error_response(wf_api::ApiError::Validation(format!(
            "at most {MAX_LOOP_BATCH_VARIABLES} entries per batch"
        )));
    }
    let mut results = Vec::with_capacity(body.entries.len());
    for entry in &body.entries {
        if entry.name.trim().is_empty() {
            results.push(BatchItemResult {
                id: entry.name.clone(),
                ok: false,
                error: Some("variable name must not be blank".to_string()),
            });
            continue;
        }
        match wf_api::agent::agent_variable::set_variable(
            &state.ctx,
            &path.id,
            &entry.name,
            entry.value.clone(),
        )
        .await
        {
            Ok(()) => results.push(BatchItemResult {
                id: entry.name.clone(),
                ok: true,
                error: None,
            }),
            Err(e) => results.push(BatchItemResult {
                id: entry.name.clone(),
                ok: false,
                error: Some(e.to_string()),
            }),
        }
    }
    ok(results).into_response()
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

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn loop_batch_reports_per_item_without_abort() {
        let ctx = make_ctx();
        // 100 mixed entries: blank names fail per item, the rest succeed.
        let entries: Vec<serde_json::Value> = (0..100)
            .map(|i| {
                if i % 2 == 0 {
                    serde_json::json!({"name": "", "value": i})
                } else {
                    serde_json::json!({"name": format!("k{i}"), "value": i})
                }
            })
            .collect();
        let response = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/agent-loops/loop-batch-1/variables/batch")
                    .header("content-type", "application/json")
                    .body(AxBody::from(
                        serde_json::to_vec(&serde_json::json!({"entries": entries})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        let items = body["data"].as_array().unwrap();
        assert_eq!(items.len(), 100);
        assert_eq!(items.iter().filter(|item| item["ok"] == true).count(), 50);
        assert_eq!(items.iter().filter(|item| item["ok"] == false).count(), 50);
    }

    #[tokio::test]
    async fn loop_batch_rejects_empty_and_oversize() {
        let ctx = make_ctx();
        let empty = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/agent-loops/loop-batch-1/variables/batch")
                    .header("content-type", "application/json")
                    .body(AxBody::from(r#"{"entries":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
        let many = format!(
            r#"{{"entries":[{}]}}"#,
            (0..101)
                .map(|i| format!(r#"{{"name":"k{i}","value":{i}}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        let oversize = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/agent-loops/loop-batch-1/variables/batch")
                    .header("content-type", "application/json")
                    .body(AxBody::from(many))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(oversize.status(), StatusCode::BAD_REQUEST);
    }
}
