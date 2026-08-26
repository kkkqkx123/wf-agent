//! Message entity surface: CRUD, stats, search, per-execution and
//! conversation queries. Handlers are thin transport adapters over the
//! `wf-api::entity::message` surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use wf_storage::adapter::message::MessageListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{ExecutionIdPath, IdPath, ListQuery};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── messages ──
        .route(
            "/messages",
            get(handle_list_messages).post(handle_save_message),
        )
        .route("/messages/stats", get(handle_message_stats))
        .route("/messages/search", get(handle_search_messages))
        .route(
            "/messages/by-execution/{executionId}",
            get(handle_messages_by_execution),
        )
        .route(
            "/messages/conversation/{executionId}",
            get(handle_conversation),
        )
        .route(
            "/messages/{id}",
            get(handle_get_message).delete(handle_delete_message),
        )
}

// ── messages ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListMessagesQuery {
    #[serde(flatten)]
    page: ListQuery,
    execution_id: Option<String>,
    agent_loop_id: Option<String>,
    role: Option<String>,
}

async fn handle_list_messages(
    State(state): State<ApiState>,
    Query(query): Query<ListMessagesQuery>,
) -> impl IntoResponse {
    let options = MessageListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        execution_id_filter: query.execution_id,
        agent_loop_id_filter: query.agent_loop_id,
        role_filter: query.role,
    };
    match wf_api::entity::message::list(&state.ctx, &options).await {
        Ok(messages) => ok(messages).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_save_message(
    State(state): State<ApiState>,
    Json(record): Json<wf_types::MessageStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::entity::message::save(&state.ctx, &record).await {
        Ok(()) => ok(record.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_message(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::message::get(&state.ctx, &path.id).await {
        Ok(message) => ok(message).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_message(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::message::delete(&state.ctx, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SearchMessagesQuery {
    q: String,
    limit: Option<usize>,
}

async fn handle_search_messages(
    State(state): State<ApiState>,
    Query(query): Query<SearchMessagesQuery>,
) -> impl IntoResponse {
    match wf_api::entity::message::search(&state.ctx, &query.q, query.limit).await {
        Ok(messages) => ok(messages).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_message_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::message::stats(&state.ctx).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ByExecutionQuery {
    offset: Option<u64>,
    limit: Option<u64>,
}

async fn handle_messages_by_execution(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
    Query(query): Query<ByExecutionQuery>,
) -> impl IntoResponse {
    match wf_api::entity::message::by_execution_paginated(
        &state.ctx,
        &path.execution_id,
        query.offset.unwrap_or(0),
        query.limit.unwrap_or(100),
        wf_api::MessageOrder::Asc,
    )
    .await
    {
        Ok(messages) => ok(messages).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_conversation(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::entity::message::conversation_history(&state.ctx, &path.execution_id).await {
        Ok(messages) => ok(messages).into_response(),
        Err(e) => error_response(e),
    }
}
