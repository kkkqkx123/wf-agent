//! Agent message and variable surfaces: per-loop messages / conversation /
//! variables. Handlers are thin transport adapters over the `wf-api::agent`
//! message and variable surfaces.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use crate::envelope::{error_response, ok};
use crate::extract::{IdNamePath, IdPath};
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
        .route(
            "/agent-loops/{id}/variables/{name}",
            get(handle_get_variable)
                .put(handle_set_variable)
                .delete(handle_delete_variable),
        )
}

// ── agent messages ────────────────────────────────────────────────

#[derive(Deserialize)]
struct RecentMessagesQuery {
    count: Option<usize>,
}

async fn handle_recent_messages(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<RecentMessagesQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_message::recent(&state.ctx, &path.id, query.count).await {
        Ok(messages) => ok(messages).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SearchMessagesQuery {
    q: String,
}

async fn handle_search_messages(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<SearchMessagesQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_message::search(&state.ctx, &path.id, &query.q).await {
        Ok(messages) => ok(messages).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_message_stats(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_message::stats(&state.ctx, &path.id).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ConversationQuery {
    max_messages: Option<usize>,
}

async fn handle_conversation(
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
        Ok(messages) => ok(messages).into_response(),
        Err(e) => error_response(e),
    }
}

// ── agent variables ───────────────────────────────────────────────

async fn handle_list_variables(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_variable::get_execution_variables(&state.ctx, &path.id).await {
        Ok(variables) => ok(variables).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_variable_stats(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_variable::get_variable_statistics(&state.ctx, &path.id).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_variable_export(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_variable::export_execution_variables(&state.ctx, &path.id).await {
        Ok(export) => ok(export).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_variable(
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

#[derive(Deserialize)]
struct SetVariableBody {
    value: Value,
}

async fn handle_set_variable(
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

async fn handle_delete_variable(
    State(state): State<ApiState>,
    Path(path): Path<IdNamePath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_variable::delete_variable(&state.ctx, &path.id, &path.name).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}
