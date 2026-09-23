//! Agent loop surface: CRUD, run control (run / stream / pause / resume /
//! cancel), status, summaries, iteration history and timeline. Handlers are
//! thin transport adapters over the `wf-api::agent` loop surfaces.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

use wf_api::AgentLoopListOptions;
use wf_api::Message;
use wf_api::{AgentLoopConfig, AgentLoopInput};

use crate::envelope::{error_response, ok};
use crate::extract::{IdNamePath, IdPath, ListQuery};
use crate::paged::{
    fetch_size, ok_capped, ok_page, resolve_page, resolve_page_fields, MAX_TIMELINE_ENTRIES,
};
use crate::router::ApiState;
use crate::sse::sse_response;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── agent loops ──
        .route(
            "/agent-loops",
            get(handle_list_loops).post(handle_save_loop),
        )
        .route(
            "/agent-loops/{id}",
            get(handle_get_loop)
                .put(handle_update_loop)
                .delete(handle_delete_loop),
        )
        .route("/agent-loops/summaries", get(handle_loop_summaries))
        .route("/agent-loops/stats", get(handle_loop_statistics))
        .route(
            "/agent-loops/{id}/status",
            patch(handle_update_loop_status).get(handle_loop_status),
        )
        .route("/agent-loops/{id}/run", post(handle_run_loop))
        .route("/agent-loops/{id}/stream", post(handle_stream_loop))
        .route("/agent-loops/{id}/pause", post(handle_pause_loop))
        .route("/agent-loops/{id}/resume", post(handle_resume_loop))
        .route("/agent-loops/{id}/cancel", post(handle_cancel_loop))
        .route(
            "/agent-loops/{id}/status/transition",
            post(handle_loop_status_transition),
        )
        .route(
            "/agent-loops/cleanup-completed",
            post(handle_cleanup_completed),
        )
        .route("/agent-loops/{id}/summary", get(handle_loop_summary))
        .route(
            "/agent-loops/{id}/iteration-history",
            get(handle_iteration_history),
        )
        .route(
            "/agent-loops/{id}/iteration-history/summary",
            get(handle_iteration_history_summary),
        )
        .route("/agent-loops/{id}/timeline", get(handle_loop_timeline))
        .route(
            "/agent-loops/{id}/variable-history/{name}",
            get(handle_variable_history),
        )
        .route(
            "/agent-loops/{id}/context-evolution",
            get(handle_loop_context_evolution),
        )
        .route(
            "/agent-loops/{id}/execution-path",
            get(handle_loop_execution_path),
        )
}

// ── agent loops ───────────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListLoopsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    status: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops",
    tag = "agent",
    params(ListLoopsQuery),
    responses(
        (status = 200, description = "List of agent loops", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 400, description = "Invalid query parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_loops(
    State(state): State<ApiState>,
    Query(query): Query<ListLoopsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    let options = AgentLoopListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        status_filter: query.status,
    };
    match wf_api::agent::agent::list_agent_loops(&state.ctx.storage, Some(options)).await {
        Ok(loops) => ok_page(loops, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops",
    tag = "agent",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Agent loop created", body = crate::envelope::ApiEnvelope<String>),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Agent loop already exists", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_loop(
    State(state): State<ApiState>,
    Json(loop_def): Json<wf_api::AgentLoopStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::agent::agent::save_agent_loop(&state.ctx.storage, &loop_def).await {
        Ok(()) => ok(loop_def.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Agent loop found", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::get_agent_loop(&state.ctx.storage, &path.id).await {
        Ok(loop_def) => ok(loop_def).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/agent-loops/{id}",
    tag = "agent",
    params(IdPath),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Agent loop updated", body = crate::envelope::ApiEnvelope<String>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut loop_def): Json<wf_api::AgentLoopStorageMetadata>,
) -> impl IntoResponse {
    loop_def.id = wf_api::Id::from(path.id.clone());
    match wf_api::agent::agent::save_agent_loop(&state.ctx.storage, &loop_def).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/agent-loops/{id}",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Agent loop deleted", body = crate::envelope::ApiEnvelope<bool>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::delete_agent_loop(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct UpdateLoopStatusBody {
    /// New status value (e.g., "running", "paused", "completed", "failed")
    status: String,
}

#[utoipa::path(
    patch,
    path = "/api/v1/agent-loops/{id}/status",
    tag = "agent",
    params(IdPath),
    request_body = UpdateLoopStatusBody,
    responses(
        (status = 200, description = "Status updated", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid status", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_loop_status(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<UpdateLoopStatusBody>,
) -> impl IntoResponse {
    match wf_api::agent::agent::update_agent_loop_status(&state.ctx.storage, &path.id, &body.status)
        .await
    {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/status",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Current loop status", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_status(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::status(&state.ctx, &path.id).await {
        Ok(status) => ok(status).into_response(),
        Err(e) => error_response(e),
    }
}

/// Live status-machine transition: when the loop is running in memory the
/// coordinator entity pauses / resumes / stops directly; otherwise the
/// persisted metadata status is rewritten (fallback of `wf-api`).
#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/status/transition",
    tag = "agent",
    params(IdPath),
    request_body = UpdateLoopStatusBody,
    responses(
        (status = 200, description = "Status transition executed", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid status", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_status_transition(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<UpdateLoopStatusBody>,
) -> impl IntoResponse {
    let status = match parse_execution_status(&body.status) {
        Ok(status) => status,
        Err(message) => {
            return crate::envelope::err(crate::envelope::ApiError::validation(message))
                .into_response()
        }
    };
    match wf_api::agent::agent_loop_registry::update_status(&state.ctx, &path.id, status).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

/// Remove all terminated (completed/failed/cancelled/stopped) live agent
/// loops from the in-memory registry.
#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/cleanup-completed",
    tag = "agent",
    responses(
        (status = 200, description = "Completed loops cleaned up", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_cleanup_completed(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::cleanup_completed(&state.ctx).await {
        Ok(count) => ok(count).into_response(),
        Err(e) => error_response(e),
    }
}

// ── agent loop run control ────────────────────────────────────────

/// Wire body of `/agent-loops/{id}/run` and `/stream`: the flattened
/// `AgentLoopConfig` fields plus the loop input.
#[derive(Deserialize, ToSchema)]
pub struct RunAgentLoopBody {
    #[serde(default)]
    agent_id: String,
    model: String,
    message: String,
    max_iterations: Option<u32>,
    max_execution_time: Option<u64>,
    hooks: Option<Vec<serde_json::Value>>,
    available_tool_names: Option<Vec<String>>,
    initial_tool_names: Option<Vec<String>>,
    discoverable_tool_names: Option<Vec<String>>,
    enable_general_tool: Option<bool>,
    hidden_tool_names: Option<Vec<String>>,
    #[schema(value_type = Object)]
    tool_call_protocol: Option<wf_api::ToolCallProtocolConfig>,
    token_limit: Option<u64>,
    token_warning_threshold: Option<u32>,
    enable_token_tracking: Option<bool>,
    /// Checkpoint every N appended conversation messages (`None` disables
    /// the message-count backstop). Wired through to
    /// `AgentLoopConfig::checkpoint_message_interval`.
    #[serde(default)]
    checkpoint_message_interval: Option<u32>,
    #[serde(default)]
    context: HashMap<String, Value>,
    #[schema(value_type = Option<Vec<Object>>)]
    conversation: Option<Vec<Message>>,
}

pub(crate) fn params_from_body(
    state: &ApiState,
    body: RunAgentLoopBody,
) -> Result<wf_api::agent::agent_execution::RunAgentLoopParams, wf_api::ApiError> {
    // Empty ids fall back to the built-in main agent, matching the CLI
    // composition path (`build_agent_loop_config` defaults `None` the same
    // way); the template resolver still rejects unregistered built-ins.
    let agent_id = if body.agent_id.trim().is_empty() {
        wf_api::DEFAULT_AGENT.to_string()
    } else {
        body.agent_id
    };
    let config = AgentLoopConfig {
        agent_id: wf_api::Id::from(agent_id),
        model: body.model,
        max_iterations: body.max_iterations,
        max_execution_time: body.max_execution_time,
        hooks: body
            .hooks
            .unwrap_or_default()
            .into_iter()
            .filter_map(|h| serde_json::from_value(h).ok())
            .collect(),
        available_tool_names: body.available_tool_names.unwrap_or_default(),
        initial_tool_names: body.initial_tool_names.unwrap_or_default(),
        discoverable_tool_names: body.discoverable_tool_names.unwrap_or_default(),
        enable_general_tool: body.enable_general_tool,
        activated_tool_names: Vec::new(),
        hidden_tool_names: body.hidden_tool_names.unwrap_or_default(),
        tool_call_protocol: body.tool_call_protocol,
        token_limit: body.token_limit,
        token_warning_threshold: body.token_warning_threshold,
        enable_token_tracking: body.enable_token_tracking,
        general_description: None,
        discoverable_metadata_block: None,
        history_normalization: false,
        checkpoint_message_interval: body.checkpoint_message_interval.filter(|n| *n > 0),
    };
    let input = AgentLoopInput {
        message: body.message,
        context: body.context,
        conversation: body.conversation.unwrap_or_default(),
    };
    // Composition boundary: resolve the agent template (built-in
    // `@standard/main` default, user overrides first) into a fully-resolved
    // config before the request reaches the execution APIs. The shared prompt
    // module renders the stable header, volatile tail and tool exposure blocks
    // from the full application context.
    let env = wf_api::PromptEnvironment::new(
        Some(state.ctx.registries.as_ref()),
        Some(state.ctx.tool_registry.as_ref()),
        state.ctx.metrics.as_deref(),
    );
    wf_api::agent::composition::resolve_run_params(&env, {
        wf_api::agent::agent_execution::RunAgentLoopParams::new(config, input)
    })
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AgentRunView {
    agent_loop_id: String,
    result: Value,
    iterations: u32,
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/run",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    request_body = RunAgentLoopBody,
    responses(
        (status = 200, description = "Agent loop execution completed", body = crate::envelope::ApiEnvelope<crate::api::agent::loops::AgentRunView>),
        (status = 404, description = "Agent loop not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_run_loop(
    State(state): State<ApiState>,
    Json(body): Json<RunAgentLoopBody>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::run(
        &state.ctx,
        match params_from_body(&state, body) {
            Ok(params) => params,
            Err(e) => return error_response(e),
        },
    )
    .await
    {
        Ok(output) => ok(AgentRunView {
            agent_loop_id: output.agent_loop_id.to_string(),
            result: output.result,
            iterations: output.iterations,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/stream",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    request_body = RunAgentLoopBody,
    responses(
        (status = 200, description = "Server-sent events stream", content_type = "text/event-stream"),
        (status = 404, description = "Agent loop not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_stream_loop(
    State(state): State<ApiState>,
    Json(body): Json<RunAgentLoopBody>,
) -> Response {
    match wf_api::agent::agent_execution::stream(
        &state.ctx,
        match params_from_body(&state, body) {
            Ok(params) => params,
            Err(e) => return error_response(e),
        },
    )
    .await
    {
        Ok(stream) => {
            let events = futures::stream::unfold(stream, |mut stream| async move {
                match stream.next().await {
                    Some(event) => {
                        let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".into());
                        let frame = format!("data: {payload}\n\n");
                        Some((
                            Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(frame)),
                            stream,
                        ))
                    }
                    None => None,
                }
            });
            sse_response(events)
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/pause",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Agent loop paused", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Cannot pause (not running)", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_pause_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::pause(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/resume",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Agent loop resumed", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Cannot resume (not paused)", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_resume_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::resume(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/cancel",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Agent loop cancelled", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Cannot cancel (not running)", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_cancel_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::cancel(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

// ── agent loop registry views ─────────────────────────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct LoopSummariesQuery {
    /// Filter by execution status
    status: Option<String>,
    /// Filter by profile ID
    profile_id: Option<String>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

/// Live-first agent loop summaries (live registry merged with persisted
/// records), filtered by optional status / profile.
#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/summaries",
    tag = "agent",
    params(LoopSummariesQuery),
    responses(
        (status = 200, description = "List of agent loop summaries", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 400, description = "Invalid query parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_summaries(
    State(state): State<ApiState>,
    Query(query): Query<LoopSummariesQuery>,
) -> impl IntoResponse {
    let status = match query
        .status
        .as_deref()
        .map(parse_execution_status)
        .transpose()
    {
        Ok(status) => status,
        Err(message) => {
            return crate::envelope::err(crate::envelope::ApiError::validation(message))
                .into_response()
        }
    };
    let filter = wf_api::AgentLoopFilter {
        ids: None,
        status,
        profile_id: query.profile_id,
        tags: None,
        created_at_range: None,
    };
    match wf_api::agent::agent_loop_registry::summaries(&state.ctx, Some(&filter)).await {
        Ok(summaries) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
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

/// Aggregate agent loop statistics (total + per-status breakdown) across
/// live and persisted loops.
#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/stats",
    tag = "agent",
    responses(
        (status = 200, description = "Agent loop statistics", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_statistics(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::statistics(&state.ctx).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

fn parse_execution_status(status: &str) -> Result<wf_types::ExecutionStatus, String> {
    serde_json::from_value(serde_json::json!(status))
        .map_err(|_| format!("unknown execution status: {status}"))
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/summary",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Agent loop summary", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_summary(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::summary(&state.ctx, &path.id).await {
        Ok(summary) => ok(summary).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/iteration-history",
    tag = "agent",
    params(IdPath, ListQuery),
    responses(
        (status = 200, description = "Iteration history", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_iteration_history(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::iteration_history(&state.ctx, &path.id).await {
        Ok(history) => {
            let (limit, offset) = resolve_page(&query);
            let window = history
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
    path = "/api/v1/agent-loops/{id}/iteration-history/summary",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Iteration history summary", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_iteration_history_summary(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::iteration_history_summary(&state.ctx, &path.id).await
    {
        Ok(summary) => ok(summary).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/timeline",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Execution timeline", body = crate::envelope::ApiEnvelope<crate::paged::CappedView<serde_json::Value>>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_timeline(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::execution_timeline(&state.ctx, &path.id).await {
        Ok(timeline) => ok_capped(timeline, MAX_TIMELINE_ENTRIES).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/variable-history/{name}",
    tag = "agent",
    params(IdNamePath, ListQuery),
    responses(
        (status = 200, description = "Variable history", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_variable_history(
    State(state): State<ApiState>,
    Path(path): Path<IdNamePath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::variable_history(&state.ctx, &path.id, &path.name)
        .await
    {
        Ok(history) => {
            let (limit, offset) = resolve_page(&query);
            let window = history
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
    path = "/api/v1/agent-loops/{id}/context-evolution",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Context evolution", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_context_evolution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::context_evolution(&state.ctx, &path.id).await {
        Ok(evolution) => ok(evolution).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/execution-path",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Execution path", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_execution_path(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::execution_path(&state.ctx, &path.id).await {
        Ok(path_view) => ok(path_view).into_response(),
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use super::{params_from_body, RunAgentLoopBody};
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use axum::response::Response;
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    fn test_state() -> crate::router::ApiState {
        crate::router::ApiState {
            ctx: make_ctx(),
            config: Arc::new(crate::middleware::ServerMiddlewareConfig::default()),
        }
    }

    async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn run_body_forwards_checkpoint_message_interval() {
        // REST backstop wiring: `checkpoint_message_interval` must reach
        // `AgentLoopConfig`; a zero value disables instead of passing
        // through (the engine treats `> 0` as enabled).
        // Async context required: `params_from_body` builds an `ApiContext`,
        // whose constructor spawns background tasks on the current runtime.
        let body = serde_json::from_value::<RunAgentLoopBody>(serde_json::json!({
            "agent_id": "agent1",
            "model": "mock",
            "message": "hi",
            "checkpoint_message_interval": 5,
        }))
        .unwrap();
        assert_eq!(
            params_from_body(&test_state(), body)
                .expect("resolve params")
                .config
                .checkpoint_message_interval,
            Some(5)
        );

        let zero = serde_json::from_value::<RunAgentLoopBody>(serde_json::json!({
            "agent_id": "agent1",
            "model": "mock",
            "message": "hi",
            "checkpoint_message_interval": 0,
        }))
        .unwrap();
        assert_eq!(
            params_from_body(&test_state(), zero)
                .expect("resolve params")
                .config
                .checkpoint_message_interval,
            None
        );

        let absent = serde_json::from_value::<RunAgentLoopBody>(serde_json::json!({
            "agent_id": "agent1",
            "model": "mock",
            "message": "hi",
        }))
        .unwrap();
        assert_eq!(
            params_from_body(&test_state(), absent)
                .expect("resolve params")
                .config
                .checkpoint_message_interval,
            None
        );
    }

    #[tokio::test]
    async fn loop_registry_summaries_and_stats_are_reachable() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/agent-loops/summaries",
            "/api/v1/agent-loops/summaries?status=running",
            "/api/v1/agent-loops/summaries?profile_id=profile-1",
            "/api/v1/agent-loops/stats",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
        // An unknown status is rejected.
        let invalid = send(ctx.clone(), "/api/v1/agent-loops/summaries?status=nope").await;
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    }
}
