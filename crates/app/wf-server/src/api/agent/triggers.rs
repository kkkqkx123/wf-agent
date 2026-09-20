//! Agent interaction surface plus the trigger execution history view.
//! Trigger definitions live in the event-driven `TriggerTemplate` registry;
//! firing records are queried through the trigger execution ledger.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── trigger execution history (ledger view) ──
        .route("/agent-triggers/history", get(handle_trigger_history))
        // ── agent interactions ──
        .route(
            "/agent-loops/{id}/interactions",
            get(handle_list_interactions),
        )
        .route("/agent-interactions/{id}", get(handle_get_interaction))
        .route(
            "/agent-interactions/{id}/respond",
            post(handle_respond_interaction),
        )
}

// ── trigger execution history (ledger view) ──

#[derive(Deserialize)]
struct TriggerHistoryQuery {
    execution_id: String,
    trigger_name: Option<String>,
}

async fn handle_trigger_history(
    State(state): State<ApiState>,
    Query(query): Query<TriggerHistoryQuery>,
) -> impl IntoResponse {
    match wf_api::entity::trigger_execution::execution_history(
        &state.ctx.storage,
        &query.execution_id,
        query.trigger_name.as_deref(),
    )
    .await
    {
        Ok(history) => ok(history).into_response(),
        Err(e) => error_response(e),
    }
}

// ── agent interactions ────────────────────────────────────────────

#[derive(Deserialize)]
struct ListInteractionsQuery {
    status: Option<String>,
    limit: Option<usize>,
}

async fn handle_list_interactions(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListInteractionsQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_user_interaction::list_filtered(
        &state.ctx,
        &path.id,
        query.status.as_deref(),
        query.limit,
    )
    .await
    {
        Ok(interactions) => ok(interactions).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_interaction(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_user_interaction::get(&state.ctx, &path.id).await {
        Ok(interaction) => ok(interaction).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct AgentRespondBody {
    agent_loop_id: Option<String>,
    response_data: Option<Value>,
    result_data: Option<Value>,
}

async fn handle_respond_interaction(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<AgentRespondBody>,
) -> impl IntoResponse {
    let Some(agent_loop_id) = body.agent_loop_id else {
        return crate::envelope::err::<Value>(crate::envelope::ApiError::validation(
            "agent_loop_id is required to respond to an agent interaction",
        ))
        .into_response();
    };
    match wf_api::agent::agent_user_interaction::respond(
        &state.ctx,
        &agent_loop_id,
        &path.id,
        body.response_data,
        body.result_data,
    )
    .await
    {
        Ok(()) => ok(()).into_response(),
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
                    .uri("/api/v1/agent-triggers/history?execution_id=exec-1")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
