//! Checkpoint domain: execution checkpoints, checkpoint CRUD / entity /
//! time-range queries and file checkpoints. Handlers are thin transport
//! adapters over the `wf-api::checkpoint` shared surfaces.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use wf_api::CheckpointListOptions;

use crate::api::workflow::executions::ExecuteView;
use crate::envelope::{error_response, ok};
use crate::extract::{CidPath, EntityIdPath, IdPath, ListQuery};
use crate::paged::{fetch_size, ok_page, resolve_page, MAX_CHAIN_ENTRIES};
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

#[utoipa::path(
    post,
    path = "/executions/{id}/checkpoints",
    tag = "checkpoint",
    params(("id" = String, Path, description = "Workflow execution ID")),
    responses(
        (status = 200, description = "Checkpoint created", body = String),
        (status = 404, description = "Execution not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid domain", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_create_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) =
        wf_api::ensure_execution_domain(&state.ctx, &path.id, wf_api::ExecutionDomain::Workflow)
            .await
    {
        return error_response(e);
    }
    match wf_api::workflow::workflow_execution::create_checkpoint(&state.ctx, &path.id).await {
        Ok(checkpoint_id) => ok(checkpoint_id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/executions/{id}/checkpoints/chain",
    tag = "checkpoint",
    params(("id" = String, Path, description = "Workflow execution ID")),
    responses(
        (status = 200, description = "Checkpoint chain analysis", body = serde_json::Value),
        (status = 404, description = "Execution not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid domain", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_checkpoint_chain(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    // Read views degrade to empty results for unknown executions instead
    // of failing; domain mismatches still surface as errors.
    if let Err(e) =
        wf_api::ensure_execution_domain(&state.ctx, &path.id, wf_api::ExecutionDomain::Workflow)
            .await
    {
        if matches!(e, wf_api::ApiError::ExecutionNotFound { .. }) {
            return ok(cap_chain(wf_api::checkpoint::record::empty_chain(&path.id)))
                .into_response();
        }
        return error_response(e);
    }
    match wf_api::checkpoint::record::chain_for_execution(&state.ctx, &path.id, None).await {
        Ok(chain) => ok(cap_chain(chain)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Capped chain view for single-execution checkpoint chains: full structure
/// up to a hard cap with an explicit truncation flag and pre-truncation
/// total.
#[derive(Serialize)]
pub(crate) struct CappedChainView {
    execution_id: String,
    checkpoints: Vec<wf_types::Checkpoint>,
    transitions: Vec<wf_api::checkpoint::record::CheckpointTransitionView>,
    total_elapsed: i64,
    checkpoint_count: usize,
    time_range: wf_api::checkpoint::record::CheckpointTimeRangeView,
    truncated: bool,
    total: usize,
}

fn cap_chain(chain: wf_api::checkpoint::record::CheckpointChainAnalysisView) -> CappedChainView {
    let total = chain.checkpoint_count;
    let truncated = total > MAX_CHAIN_ENTRIES;
    let mut checkpoints = chain.checkpoints;
    let mut transitions = chain.transitions;
    checkpoints.truncate(MAX_CHAIN_ENTRIES);
    transitions.truncate(MAX_CHAIN_ENTRIES);
    let checkpoint_count = checkpoints.len();
    CappedChainView {
        execution_id: chain.execution_id,
        checkpoints,
        transitions,
        total_elapsed: chain.total_elapsed,
        checkpoint_count,
        time_range: chain.time_range,
        truncated,
        total,
    }
}

#[utoipa::path(
    post,
    path = "/executions/checkpoints/{cid}/restore",
    tag = "checkpoint",
    params(("cid" = String, Path, description = "Checkpoint ID")),
    responses(
        (status = 200, description = "Checkpoint restored", body = serde_json::Value),
        (status = 404, description = "Checkpoint not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_restore_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<CidPath>,
) -> impl IntoResponse {
    if let Err(e) = wf_api::checkpoint::ensure_checkpoint_domain(
        &state.ctx,
        &path.cid,
        wf_api::ExecutionDomain::Workflow,
    )
    .await
    {
        return error_response(e);
    }
    match wf_api::workflow::workflow_execution::restore_checkpoint(&state.ctx, &path.cid).await {
        Ok(restored) => ok(restored).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/executions/checkpoints/{cid}/resume",
    tag = "checkpoint",
    params(("cid" = String, Path, description = "Checkpoint ID")),
    responses(
        (status = 200, description = "Execution resumed from checkpoint", body = serde_json::Value),
        (status = 404, description = "Checkpoint not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_restore_and_resume(
    State(state): State<ApiState>,
    Path(path): Path<CidPath>,
) -> impl IntoResponse {
    if let Err(e) = wf_api::checkpoint::ensure_checkpoint_domain(
        &state.ctx,
        &path.cid,
        wf_api::ExecutionDomain::Workflow,
    )
    .await
    {
        return error_response(e);
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
pub(crate) struct ListCheckpointsQuery {
    #[serde(flatten)]
    page: ListQuery,
    /// Filter by entity ID
    entity_id: Option<String>,
    /// Filter by entity type
    entity_type: Option<String>,
}

#[utoipa::path(
    get,
    path = "/checkpoints",
    tag = "checkpoint",
    params(("limit" = Option<u64>, Query, description = "Page limit"), ("offset" = Option<u64>, Query, description = "Page offset")),
    responses(
        (status = 200, description = "List of checkpoints", body = serde_json::Value),
        (status = 400, description = "Invalid query parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_list_checkpoints(
    State(state): State<ApiState>,
    Query(query): Query<ListCheckpointsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    let options = CheckpointListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        entity_type_filter: query.entity_type,
        entity_id_filter: query.entity_id,
    };
    match wf_api::checkpoint::record::list_checkpoints(&state.ctx.storage, Some(options)).await {
        Ok(checkpoints) => ok_page(checkpoints, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/checkpoints/{id}",
    tag = "checkpoint",
    params(("id" = String, Path, description = "Checkpoint ID")),
    responses(
        (status = 200, description = "Checkpoint found", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_get_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::record::get_checkpoint(&state.ctx.storage, &path.id).await {
        Ok(checkpoint) => ok(checkpoint).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/checkpoints/{id}",
    tag = "checkpoint",
    params(("id" = String, Path, description = "Checkpoint ID")),
    responses(
        (status = 200, description = "Checkpoint deleted", body = bool),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_delete_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::checkpoint::record::delete_checkpoint(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/checkpoints/entity/{entityId}",
    tag = "checkpoint",
    params(
        ("entityId" = String, Path, description = "Entity ID"), ("limit" = Option<u64>, Query, description = "Page limit"), ("offset" = Option<u64>, Query, description = "Page offset")),
    responses(
        (status = 200, description = "List of checkpoints for entity", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_list_checkpoints_by_entity(
    State(state): State<ApiState>,
    Path(path): Path<EntityIdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::checkpoint::record::list_checkpoints_by_entity(
        &state.ctx.storage,
        &path.entity_id,
        "checkpoint",
    )
    .await
    {
        Ok(checkpoints) => {
            let (limit, offset) = resolve_page(&query);
            let window = checkpoints
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
    path = "/checkpoints/entity/{entityId}/latest",
    tag = "checkpoint",
    params(("entityId" = String, Path, description = "Entity ID")),
    responses(
        (status = 200, description = "Latest checkpoint", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_latest_checkpoint(
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
pub(crate) struct DeleteCheckpointsQuery {
    entity_type: Option<String>,
}

#[utoipa::path(
    delete,
    path = "/checkpoints/entity/{entityId}",
    tag = "checkpoint",
    params(("entityId" = String, Path, description = "Entity ID")),
    responses(
        (status = 200, description = "Checkpoints deleted", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_delete_checkpoints_by_entity(
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

#[utoipa::path(
    get,
    path = "/checkpoints/entity/{entityId}/metadata",
    tag = "checkpoint",
    params(("entityId" = String, Path, description = "Entity ID")),
    responses(
        (status = 200, description = "Entity metadata", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_checkpoint_entity_metadata(
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

#[utoipa::path(
    put,
    path = "/checkpoints/entity/{entityId}/metadata",
    tag = "checkpoint",
    params(("entityId" = String, Path, description = "Entity ID")),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Metadata updated", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_set_checkpoint_entity_metadata(
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
pub(crate) struct CheckpointEntitiesQuery {
    entity_ids: String,
    entity_type: Option<String>,
    #[serde(flatten)]
    page: ListQuery,
}

#[utoipa::path(
    get,
    path = "/checkpoints/entities",
    tag = "checkpoint",
    params(("limit" = Option<u64>, Query, description = "Page limit"), ("offset" = Option<u64>, Query, description = "Page offset")),
    responses(
        (status = 200, description = "Checkpoints by entities", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_list_checkpoints_by_entities(
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
        Ok(checkpoints) => {
            let (limit, offset) = resolve_page(&query.page);
            let window = checkpoints
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CheckpointsTimeRangeQuery {
    workflow_id: String,
    start: i64,
    end: i64,
    #[serde(flatten)]
    page: ListQuery,
}

#[utoipa::path(
    get,
    path = "/checkpoints/time-range",
    tag = "checkpoint",
    params(("limit" = Option<u64>, Query, description = "Page limit"), ("offset" = Option<u64>, Query, description = "Page offset")),
    responses(
        (status = 200, description = "Checkpoints by time range", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_checkpoints_by_time_range(
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
        Ok(checkpoints) => {
            let (limit, offset) = resolve_page(&query.page);
            let window = checkpoints
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}
