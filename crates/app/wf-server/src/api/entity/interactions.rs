//! Entity interaction surface: agent-loop scoped user interactions.
//! Split out of the legacy agent trigger file so the trigger domain owns
//! only the execution ledger (`trigger/executions.rs`) and the webhook
//! gateway (`trigger/hooks.rs`).

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
