//! Checkpoint domain: execution checkpoints, checkpoint CRUD / entity /
//! time-range queries and file checkpoints. Handlers are thin transport
//! adapters over the `wf-api::workflow` checkpoint surfaces.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_storage::adapter::checkpoint::CheckpointListOptions;

use crate::api::workflow::executions::ExecuteView;
use crate::envelope::{error_response, ok};
use crate::extract::{CidPath, EntityIdPath, IdPath, ListQuery};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── execution checkpoints ──
        .route(
            "/executions/{id}/checkpoints",
            post(handle_create_checkpoint),
        )
        .route(
            "/executions/{id}/checkpoints/chain",
            get(handle_checkpoint_chain),
        )
        .route(
            "/executions/checkpoints/{cid}/restore",
            post(handle_restore_checkpoint),
        )
        .route(
            "/executions/checkpoints/{cid}/resume",
            post(handle_restore_and_resume),
        )
        // ── checkpoints ──
        .route("/checkpoints", get(handle_list_checkpoints))
        .route(
            "/checkpoints/{id}",
            get(handle_get_checkpoint).delete(handle_delete_checkpoint),
        )
        .route(
            "/checkpoints/entity/{entityId}",
            get(handle_list_checkpoints_by_entity).delete(handle_delete_checkpoints_by_entity),
        )
        .route(
            "/checkpoints/entity/{entityId}/metadata",
            get(handle_checkpoint_entity_metadata).put(handle_set_checkpoint_entity_metadata),
        )
        .route(
            "/checkpoints/entities",
            get(handle_list_checkpoints_by_entities),
        )
        .route(
            "/checkpoints/time-range",
            get(handle_checkpoints_by_time_range),
        )
        .route(
            "/checkpoints/entity/{entityId}/latest",
            get(handle_latest_checkpoint),
        )
}

// ── checkpoints ───────────────────────────────────────────────────

async fn handle_create_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = wf_api::ensure_execution_domain(
        &state.ctx,
        &path.id,
        wf_api::ExecutionDomain::Workflow,
    )
    .await
    {
        return error_response(e);
    }
    match wf_api::workflow::workflow_execution::create_checkpoint(&state.ctx, &path.id).await {
        Ok(checkpoint_id) => ok(checkpoint_id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_checkpoint_chain(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = wf_api::ensure_execution_domain(
        &state.ctx,
        &path.id,
        wf_api::ExecutionDomain::Workflow,
    )
    .await
    {
        return error_response(e);
    }
    match wf_api::checkpoint::record::chain_for_execution(&state.ctx, &path.id, None).await {
        Ok(chain) => ok(chain).into_response(),
        Err(e) => error_response(e),
    }
}

async fn reject_agent_checkpoint(
    state: &ApiState,
    cid: &str,
) -> Option<axum::response::Response> {
    match wf_api::checkpoint::record::get_checkpoint(&state.ctx.storage, cid).await {
        Ok(cp) if cp.entity_type == "agent_loop" => Some(error_response(
            wf_api::ApiError::Validation(format!(
                "checkpoint [{cid}] belongs to agent_loop, not workflow; use the agent-loop checkpoint endpoint"
            )),
        )),
        Ok(_) => None,
        Err(e) => match e {
            wf_api::ApiError::NotFound { .. } => Some(error_response(e)),
            _ => Some(error_response(e)),
        },
    }
}

async fn handle_restore_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<CidPath>,
) -> impl IntoResponse {
    if let Some(response) = reject_agent_checkpoint(&state, &path.cid).await {
        return response;
    }
    match wf_api::workflow::workflow_execution::restore_checkpoint(&state.ctx, &path.cid).await {
        Ok(restored) => ok(restored).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_restore_and_resume(
    State(state): State<ApiState>,
    Path(path): Path<CidPath>,
) -> impl IntoResponse {
    if let Some(response) = reject_agent_checkpoint(&state, &path.cid).await {
        return response;
    }
    match wf_api::workflow::workflow_execution::restore_and_resume(&state.ctx, &path.cid).await {
        Ok(output) => ok(ExecuteView {
            execution_id: output.execution_id.to_string(),
            result: output.result,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ListCheckpointsQuery {
    #[serde(flatten)]
    page: ListQuery,
    entity_id: Option<String>,
    entity_type: Option<String>,
}

async fn handle_list_checkpoints(
    State(state): State<ApiState>,
    Query(query): Query<ListCheckpointsQuery>,
) -> impl IntoResponse {
    let options = CheckpointListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        entity_type_filter: query.entity_type,
        entity_id_filter: query.entity_id,
    };
    match wf_api::checkpoint::record::list_checkpoints(&state.ctx.storage, Some(options)).await {
        Ok(checkpoints) => ok(checkpoints).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::record::get_checkpoint(&state.ctx.storage, &path.id).await {
        Ok(checkpoint) => ok(checkpoint).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::record::delete_checkpoint(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_list_checkpoints_by_entity(
    State(state): State<ApiState>,
    Path(path): Path<EntityIdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::record::list_checkpoints_by_entity(
        &state.ctx.storage,
        &path.entity_id,
        "checkpoint",
    )
    .await
    {
        Ok(checkpoints) => ok(checkpoints).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_latest_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<EntityIdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::record::get_latest_checkpoint(
        &state.ctx.storage,
        &path.entity_id,
        "checkpoint",
    )
    .await
    {
        Ok(checkpoint) => ok(checkpoint).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct DeleteCheckpointsQuery {
    entity_type: Option<String>,
}

async fn handle_delete_checkpoints_by_entity(
    State(state): State<ApiState>,
    Path(path): Path<EntityIdPath>,
    Query(query): Query<DeleteCheckpointsQuery>,
) -> impl IntoResponse {
    let entity_type = query.entity_type.as_deref().unwrap_or("checkpoint");
    match wf_api::checkpoint::record::delete_checkpoints_by_entity(
        &state.ctx.storage,
        &path.entity_id,
        entity_type,
    )
    .await
    {
        Ok(removed) => ok(removed).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_checkpoint_entity_metadata(
    State(state): State<ApiState>,
    Path(path): Path<EntityIdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::record::get_checkpoint_entity_metadata(
        &state.ctx.storage,
        &path.entity_id,
    )
    .await
    {
        Ok(metadata) => ok(metadata).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_set_checkpoint_entity_metadata(
    State(state): State<ApiState>,
    Path(path): Path<EntityIdPath>,
    Json(metadata): Json<std::collections::HashMap<String, Value>>,
) -> impl IntoResponse {
    match wf_api::checkpoint::record::set_checkpoint_entity_metadata(
        &state.ctx.storage,
        &path.entity_id,
        &metadata,
    )
    .await
    {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct CheckpointEntitiesQuery {
    entity_ids: String,
    entity_type: Option<String>,
}

async fn handle_list_checkpoints_by_entities(
    State(state): State<ApiState>,
    Query(query): Query<CheckpointEntitiesQuery>,
) -> impl IntoResponse {
    let ids: Vec<String> = query
        .entity_ids
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let entity_type = query.entity_type.as_deref().unwrap_or("checkpoint");
    match wf_api::checkpoint::record::list_checkpoints_by_entities(
        &state.ctx.storage,
        &ids,
        entity_type,
    )
    .await
    {
        Ok(checkpoints) => ok(checkpoints).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointsTimeRangeQuery {
    workflow_id: String,
    start: i64,
    end: i64,
}

async fn handle_checkpoints_by_time_range(
    State(state): State<ApiState>,
    Query(query): Query<CheckpointsTimeRangeQuery>,
) -> impl IntoResponse {
    match wf_api::checkpoint::record::list_checkpoints_by_time_range(
        &state.ctx.storage,
        &query.workflow_id,
        query.start,
        query.end,
    )
    .await
    {
        Ok(checkpoints) => ok(checkpoints).into_response(),
        Err(e) => error_response(e),
    }
}
