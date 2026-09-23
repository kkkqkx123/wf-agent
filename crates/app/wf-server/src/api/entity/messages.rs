//! Message entity surface: CRUD, stats, search, per-execution and
//! conversation queries. Handlers are thin transport adapters over the
//! `wf-api::entity::message` surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use wf_api::MessageListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{ExecutionIdPath, IdPath, ListQuery};
use crate::paged::{fetch_size, ok_page, resolve_page};
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
    let (limit, offset) = resolve_page(&query.page);
    let options = MessageListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        before_timestamp: None,
        execution_id_filter: query.execution_id,
        agent_loop_id_filter: query.agent_loop_id,
        role_filter: query.role,
    };
    match wf_api::entity::message::list(&state.ctx, &options).await {
        Ok(messages) => ok_page(messages, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_save_message(
    State(state): State<ApiState>,
    Json(record): Json<wf_api::MessageStorageMetadata>,
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
    #[serde(flatten)]
    page: ListQuery,
}

async fn handle_search_messages(
    State(state): State<ApiState>,
    Query(query): Query<SearchMessagesQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    // Search sorts newest-first in the domain; page over the limited window.
    let fetch = offset.saturating_add(limit).saturating_add(1).min(500) as usize;
    match wf_api::entity::message::search(&state.ctx, &query.q, Some(fetch)).await {
        Ok(messages) => {
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

async fn handle_message_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::message::stats(&state.ctx).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ByExecutionQuery {
    #[serde(flatten)]
    page: ListQuery,
}

async fn handle_messages_by_execution(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
    Query(query): Query<ByExecutionQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    match wf_api::entity::message::by_execution_paginated(
        &state.ctx,
        &path.execution_id,
        offset,
        fetch_size(limit),
        wf_api::MessageOrder::Asc,
    )
    .await
    {
        Ok(messages) => ok_page(messages, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ConversationQuery {
    #[serde(flatten)]
    page: ListQuery,
}

async fn handle_conversation(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
    Query(query): Query<ConversationQuery>,
) -> impl IntoResponse {
    match wf_api::entity::message::conversation_history(&state.ctx, &path.execution_id).await {
        Ok(messages) => {
            let (limit, offset) = resolve_page(&query.page);
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
