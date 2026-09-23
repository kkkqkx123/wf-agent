//! Agent execution surface: agent execution records and agent-loop
//! checkpoints. Handlers are thin transport adapters over the
//! `wf-api::agent` execution and checkpoint surfaces.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::extract::{DefIdPath, IdPath, ListQuery};
use crate::paged::{fetch_size, ok_capped, ok_page, resolve_page, MAX_CHAIN_ENTRIES};
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
            "/agent-loops/{id}/checkpoints/{cid}/resume",
            post(handle_resume_checkpoint),
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
pub(crate) struct AgentExecutionsQuery {
    #[serde(flatten)]
    page: ListQuery,
    status: Option<String>,
    agent_id: Option<String>,
}

#[utoipa::path(
    get,
    path = "/agent-executions",
    tag = "agent",
    params(("limit" = Option<u64>, Query, description = "Page limit"), ("offset" = Option<u64>, Query, description = "Page offset")),
    responses(
        (status = 200, description = "List of agent executions", body = serde_json::Value),
        (status = 400, description = "Invalid query parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_agent_executions(
    State(state): State<ApiState>,
    Query(query): Query<AgentExecutionsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    let filter = wf_api::AgentExecutionFilter {
        status: query
            .status
            .as_deref()
            .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        agent_id: query.agent_id,
        parent_execution_id: None,
    };
    match wf_api::agent::agent_execution_registry::summaries(&state.ctx, Some(&filter)).await {
        Ok(summaries) => {
            let window = summaries
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
    path = "/agent-executions/{id}",
    tag = "agent",
    params(("id" = String, Path, description = "Agent execution ID")),
    responses(
        (status = 200, description = "Agent execution found", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_get_agent_execution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::get_agent_execution(&state.ctx.storage, &path.id).await {
        Ok(execution) => ok(execution).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/agent-executions/{id}",
    tag = "agent",
    params(("id" = String, Path, description = "Agent execution ID")),
    responses(
        (status = 200, description = "Agent execution deleted", body = bool),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_delete_agent_execution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::delete_agent_execution(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-executions/by-definition/{defId}",
    tag = "agent",
    params(
        ("defId" = String, Path, description = "Agent definition ID"), ("limit" = Option<u64>, Query, description = "Page limit"), ("offset" = Option<u64>, Query, description = "Page offset")),
    responses(
        (status = 200, description = "List of executions for definition", body = serde_json::Value),
        (status = 400, description = "Invalid query parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_executions_by_definition(
    State(state): State<ApiState>,
    Path(path): Path<DefIdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query);
    match wf_api::agent::agent::list_executions_by_definition(&state.ctx.storage, &path.def_id)
        .await
    {
        Ok(executions) => {
            let window = executions
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
    path = "/agent-executions/stats",
    tag = "agent",
    responses(
        (status = 200, description = "Agent execution statistics", body = serde_json::Value),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_execution_statistics(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution_registry::execution_statistics(&state.ctx).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-executions/by-status/{status}",
    tag = "agent",
    params(
        ("status" = String, Path, description = "Execution status (running, paused, completed, failed)"), ("limit" = Option<u64>, Query, description = "Page limit"), ("offset" = Option<u64>, Query, description = "Page offset")),
    responses(
        (status = 200, description = "Executions by status", body = serde_json::Value),
        (status = 400, description = "Invalid status", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_executions_by_status(
    State(state): State<ApiState>,
    Path(path): Path<crate::extract::StatusPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query);
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
        Ok(summaries) => {
            let window = summaries
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

// ── agent checkpoints ─────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateCheckpointBody {
    /// Optional description for the checkpoint
    description: Option<String>,
}

async fn ensure_agent_domain(
    ctx: &wf_api::ApiContext,
    id: &str,
) -> Result<wf_api::ExecutionDomain, wf_api::ApiError> {
    wf_api::ensure_execution_domain(ctx, id, wf_api::ExecutionDomain::AgentLoop).await
}

#[utoipa::path(
    post,
    path = "/agent-loops/{id}/checkpoints",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Checkpoint created", body = serde_json::Value),
        (status = 404, description = "Agent loop not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_create_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<CreateCheckpointBody>,
) -> impl IntoResponse {
    if let Err(e) = ensure_agent_domain(&state.ctx, &path.id).await {
        return error_response(e);
    }
    match wf_api::agent::agent_checkpoint::create(&state.ctx, &path.id, body.description).await {
        Ok(checkpoint) => ok(checkpoint).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/checkpoints",
    tag = "agent",
    params(
        ("id" = String, Path, description = "Agent loop ID"), ("limit" = Option<u64>, Query, description = "Page limit"), ("offset" = Option<u64>, Query, description = "Page offset")),
    responses(
        (status = 200, description = "List of checkpoints", body = serde_json::Value),
        (status = 404, description = "Agent loop not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_list_checkpoints(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    if let Err(e) = ensure_agent_domain(&state.ctx, &path.id).await {
        return error_response(e);
    }
    let (limit, offset) = resolve_page(&query);
    match wf_api::agent::agent_checkpoint::list(&state.ctx, &path.id).await {
        Ok(checkpoints) => {
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
    post,
    path = "/agent-loops/{id}/checkpoints/{cid}/restore",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID"), ("cid" = String, Path, description = "Checkpoint ID")),
    responses(
        (status = 200, description = "Checkpoint restored", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_restore_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<crate::extract::IdCidPath>,
) -> impl IntoResponse {
    if let Err(e) = ensure_agent_domain(&state.ctx, &path.id).await {
        return error_response(e);
    }
    if let Err(e) = wf_api::checkpoint::ensure_checkpoint_domain(
        &state.ctx,
        &path.cid,
        wf_api::ExecutionDomain::AgentLoop,
    )
    .await
    {
        return error_response(e);
    }
    match wf_api::agent::agent_checkpoint::restore(&state.ctx, &path.id, &path.cid).await {
        Ok(checkpoint) => ok(checkpoint).into_response(),
        Err(e) => error_response(e),
    }
}

/// Resume mode for checkpoint resume: `branch` (default) continues under a
/// fresh execution id; `in_place` continues under the source execution id.
#[derive(Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResumeCheckpointMode {
    #[default]
    Branch,
    InPlace,
}

#[derive(Deserialize)]
pub(crate) struct ResumeCheckpointBody {
    #[serde(default)]
    mode: ResumeCheckpointMode,
    #[serde(flatten)]
    run: super::loops::RunAgentLoopBody,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AgentResumeView {
    agent_loop_id: String,
    result: Value,
    iterations: u32,
}

/// Resume an agent loop from one of its engine checkpoints and run it to
/// completion: `mode: "branch"` (default) forks under a fresh execution id,
/// `mode: "in_place"` continues under the source execution id (the source
/// must be terminal or paused). The remaining body fields mirror
/// `/agent-loops/{id}/run` (model, message, hooks, tool visibility, ...).
#[utoipa::path(
    post,
    path = "/agent-loops/{id}/checkpoints/{cid}/resume",
    tag = "agent",
    params(
        ("id" = String, Path, description = "Agent loop ID"),
        ("cid" = String, Path, description = "Checkpoint ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Agent loop resumed from checkpoint", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Invalid mode for checkpoint state", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_resume_checkpoint(
    State(state): State<ApiState>,
    Path(path): Path<crate::extract::IdCidPath>,
    Json(body): Json<ResumeCheckpointBody>,
) -> impl IntoResponse {
    // The checkpoint is validated first: resuming from a checkpoint that
    // does not exist is an invalid request (400), never a routing failure.
    // A checkpoint that exists but belongs to another domain stays a
    // domain-mismatch error from the ownership check below.
    if let Err(e) = wf_api::checkpoint::ensure_checkpoint_domain(
        &state.ctx,
        &path.cid,
        wf_api::ExecutionDomain::AgentLoop,
    )
    .await
    {
        return error_response(match e {
            wf_api::ApiError::NotFound { .. } => {
                wf_api::ApiError::Validation(format!("unknown checkpoint id [{}]", path.cid))
            }
            other => other,
        });
    }
    if let Err(e) = ensure_agent_domain(&state.ctx, &path.id).await {
        return error_response(e);
    }
    let in_place = body.mode == ResumeCheckpointMode::InPlace;
    let run_params = match super::loops::params_from_body(&state, body.run) {
        Ok(params) => params,
        Err(e) => return error_response(e),
    };
    match wf_api::agent::agent_execution::resume_from_checkpoint(
        &state.ctx, &path.id, &path.cid, run_params, in_place,
    )
    .await
    {
        Ok(output) => ok(AgentResumeView {
            agent_loop_id: output.agent_loop_id.to_string(),
            result: output.result,
            iterations: output.iterations,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/checkpoints/chain",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Checkpoint chain", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_checkpoint_chain(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = ensure_agent_domain(&state.ctx, &path.id).await {
        return error_response(e);
    }
    match wf_api::agent::agent_checkpoint::chain(&state.ctx, &path.id).await {
        Ok(chain) => ok_capped(chain, MAX_CHAIN_ENTRIES).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/agent-loops/{id}/checkpoints",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Checkpoints deleted", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_delete_checkpoints(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = ensure_agent_domain(&state.ctx, &path.id).await {
        return error_response(e);
    }
    match wf_api::agent::agent_checkpoint::delete_for(&state.ctx, &path.id).await {
        Ok(removed) => ok(removed).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-checkpoints/stats",
    tag = "agent",
    responses(
        (status = 200, description = "Checkpoint statistics", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_checkpoint_statistics(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    match wf_api::agent::agent_checkpoint::statistics(&state.ctx, None).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    #[tokio::test]
    async fn resume_checkpoint_route_rejects_unknown_checkpoint() {
        // The route exists and reaches the ownership check without any LLM
        // involvement: an unknown checkpoint id is a client error, never a
        // routing failure.
        let app = crate::router::api_router(make_ctx());
        let body = serde_json::json!({
            "mode": "in_place",
            "agent_id": "agent-1",
            "model": "mock",
            "message": "hi",
        });
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/agent-loops/loop-1/checkpoints/cid-1/resume")
            .header("content-type", "application/json")
            .body(AxBody::from(serde_json::to_string(&body).unwrap()))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
        assert!(
            response.status().is_client_error(),
            "unknown checkpoint must be a client error, got {}",
            response.status()
        );
    }
}
