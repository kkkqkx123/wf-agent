//! Agent execution surface: agent execution records and agent-loop
//! checkpoints. Handlers are thin transport adapters over the
//! `wf-api::agent` execution and checkpoint surfaces.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_types::checkpoint::base::CheckpointType;

use crate::envelope::{error_response, ok};
use crate::extract::{DefIdPath, IdPath};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── agent executions ──
        .route("/agent-executions", get(handle_agent_executions))
        .route(
            "/agent-executions/{id}",
            get(handle_get_agent_execution).delete(handle_delete_agent_execution),
        )
        .route(
            "/agent-executions/by-definition/{defId}",
            get(handle_executions_by_definition),
        )
        .route("/agent-executions/stats", get(handle_execution_statistics))
        .route(
            "/agent-executions/by-status/{status}",
            get(handle_executions_by_status),
        )
        // ── agent checkpoints ──
        .route(
            "/agent-loops/{id}/checkpoints",
            get(handle_list_checkpoints).post(handle_create_checkpoint),
        )
        .route(
            "/agent-loops/{id}/checkpoints/{cid}/restore",
            post(handle_restore_checkpoint),
        )
        .route(
            "/agent-loops/{id}/checkpoints/chain",
            get(handle_checkpoint_chain),
        )
        .route(
            "/agent-loops/{id}/checkpoints",
            delete(handle_delete_checkpoints),
        )
        .route(
            "/agent-checkpoints/stats",
            get(handle_checkpoint_statistics),
        )
}

// ── agent executions ──────────────────────────────────────────────

#[derive(Deserialize)]
struct AgentExecutionsQuery {
    status: Option<String>,
    agent_id: Option<String>,
}

async fn handle_agent_executions(
    State(state): State<ApiState>,
    Query(query): Query<AgentExecutionsQuery>,
) -> impl IntoResponse {
    let filter = wf_api::AgentExecutionFilter {
        status: query
            .status
            .as_deref()
            .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        agent_id: query.agent_id,
        parent_execution_id: None,
    };
    match wf_api::agent::agent_execution_registry::summaries(&state.ctx, Some(&filter)).await {
        Ok(summaries) => ok(summaries).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_agent_execution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::get_agent_execution(&state.ctx.storage, &path.id).await {
        Ok(execution) => ok(execution).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_agent_execution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::delete_agent_execution(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_executions_by_definition(
    State(state): State<ApiState>,
    Path(path): Path<DefIdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::list_executions_by_definition(&state.ctx.storage, &path.def_id)
        .await
    {
        Ok(executions) => ok(executions).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_execution_statistics(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::agent::agent_execution_registry::execution_statistics(&state.ctx).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_executions_by_status(
    State(state): State<ApiState>,
    Path(path): Path<crate::extract::StatusPath>,
) -> impl IntoResponse {
    let result = match path.status.as_str() {
        "running" => wf_api::agent::agent_execution_registry::running(&state.ctx).await,
        "paused" => wf_api::agent::agent_execution_registry::paused(&state.ctx).await,
        "completed" => wf_api::agent::agent_execution_registry::completed(&state.ctx).await,
        "failed" => wf_api::agent::agent_execution_registry::failed(&state.ctx).await,
        other => {
            return crate::envelope::err::<Value>(crate::envelope::ApiError::validation(format!(
                "unknown agent execution status: {other}"
            )))
            .into_response()
        }
    };
    match result {
        Ok(summaries) => ok(summaries).into_response(),
        Err(e) => error_response(e),
    }
}

// ── agent checkpoints ─────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateCheckpointBody {
    checkpoint_type: Option<CheckpointType>,
    tags: Option<Vec<String>>,
}

async fn handle_create_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<CreateCheckpointBody>,
) -> impl IntoResponse {
    match wf_api::agent::agent_checkpoint::create(
        &state.ctx,
        &path.id,
        body.checkpoint_type.unwrap_or(CheckpointType::Full),
        body.tags,
    )
    .await
    {
        Ok(checkpoint) => ok(checkpoint).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_list_checkpoints(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_checkpoint::list(&state.ctx, &path.id).await {
        Ok(checkpoints) => ok(checkpoints).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_restore_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<crate::extract::IdCidPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_checkpoint::restore(&state.ctx, &path.id, &path.cid).await {
        Ok(checkpoint) => ok(checkpoint).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_checkpoint_chain(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_checkpoint::chain(&state.ctx, &path.id).await {
        Ok(chain) => ok(chain).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_checkpoints(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_checkpoint::delete_for(&state.ctx, &path.id).await {
        Ok(removed) => ok(removed).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_checkpoint_statistics(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::agent::agent_checkpoint::statistics(&state.ctx, None).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}
