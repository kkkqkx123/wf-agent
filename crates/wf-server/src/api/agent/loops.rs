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

use wf_storage::adapter::agent_loop::AgentLoopListOptions;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_types::message::Message;

use crate::envelope::{error_response, ok};
use crate::extract::{IdNamePath, IdPath, ListQuery};
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

#[derive(Deserialize)]
struct ListLoopsQuery {
    #[serde(flatten)]
    page: ListQuery,
    status: Option<String>,
}

async fn handle_list_loops(
    State(state): State<ApiState>,
    Query(query): Query<ListLoopsQuery>,
) -> impl IntoResponse {
    let options = AgentLoopListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        status_filter: query.status,
    };
    match wf_api::agent::agent::list_agent_loops(&state.ctx.storage, Some(options)).await {
        Ok(loops) => ok(loops).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_save_loop(
    State(state): State<ApiState>,
    Json(loop_def): Json<wf_types::AgentLoopStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::agent::agent::save_agent_loop(&state.ctx.storage, &loop_def).await {
        Ok(()) => ok(loop_def.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::get_agent_loop(&state.ctx.storage, &path.id).await {
        Ok(loop_def) => ok(loop_def).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_update_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut loop_def): Json<wf_types::AgentLoopStorageMetadata>,
) -> impl IntoResponse {
    loop_def.id = wf_types::Id::from(path.id.clone());
    match wf_api::agent::agent::save_agent_loop(&state.ctx.storage, &loop_def).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::delete_agent_loop(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct UpdateLoopStatusBody {
    status: String,
}

async fn handle_update_loop_status(
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

async fn handle_loop_status(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::status(&state.ctx, &path.id).await {
        Ok(status) => ok(status).into_response(),
        Err(e) => error_response(e),
    }
}

// ── agent loop run control ────────────────────────────────────────

/// Wire body of `/agent-loops/{id}/run` and `/stream`: the flattened
/// `AgentLoopConfig` fields plus the loop input.
#[derive(Deserialize)]
pub struct RunAgentLoopBody {
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
    tool_call_format: Option<wf_types::llm::tool_call_format::ToolCallFormatConfig>,
    token_limit: Option<u64>,
    token_warning_threshold: Option<u32>,
    enable_token_tracking: Option<bool>,
    #[serde(default)]
    context: HashMap<String, Value>,
    conversation: Option<Vec<Message>>,
}

fn params_from_body(body: RunAgentLoopBody) -> wf_api::agent::agent_execution::RunAgentLoopParams {
    let config = AgentLoopConfig {
        agent_id: wf_types::Id::from(body.agent_id),
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
        tool_call_format: body.tool_call_format,
        token_limit: body.token_limit,
        token_warning_threshold: body.token_warning_threshold,
        enable_token_tracking: body.enable_token_tracking,
        general_description: None,
        discoverable_metadata_block: None,
    };
    let input = AgentLoopInput {
        message: body.message,
        context: body.context,
        conversation: body.conversation.unwrap_or_default(),
    };
    wf_api::agent::agent_execution::RunAgentLoopParams { config, input }
}

#[derive(Serialize)]
struct AgentRunView {
    agent_loop_id: String,
    result: Value,
    iterations: u32,
}

async fn handle_run_loop(
    State(state): State<ApiState>,
    Json(body): Json<RunAgentLoopBody>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::run(&state.ctx, params_from_body(body)).await {
        Ok(output) => ok(AgentRunView {
            agent_loop_id: output.agent_loop_id.to_string(),
            result: output.result,
            iterations: output.iterations,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_stream_loop(
    State(state): State<ApiState>,
    Json(body): Json<RunAgentLoopBody>,
) -> Response {
    match wf_api::agent::agent_execution::stream(&state.ctx, params_from_body(body)).await {
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

async fn handle_pause_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::pause(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_resume_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::resume(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_cancel_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::cancel(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

// ── agent loop registry views ─────────────────────────────────────

#[derive(Deserialize)]
struct LoopSummariesQuery {
    status: Option<String>,
    profile_id: Option<String>,
}

/// Live-first agent loop summaries (live registry merged with persisted
/// records), filtered by optional status / profile.
async fn handle_loop_summaries(
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
            return crate::envelope::err::<serde_json::Value>(
                crate::envelope::ApiError::validation(message),
            )
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
        Ok(summaries) => ok(summaries).into_response(),
        Err(e) => error_response(e),
    }
}

/// Aggregate agent loop statistics (total + per-status breakdown) across
/// live and persisted loops.
async fn handle_loop_statistics(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::statistics(&state.ctx).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

fn parse_execution_status(status: &str) -> Result<wf_types::ExecutionStatus, String> {
    serde_json::from_value(serde_json::json!(status))
        .map_err(|_| format!("unknown execution status: {status}"))
}

async fn handle_loop_summary(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::summary(&state.ctx, &path.id).await {
        Ok(summary) => ok(summary).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_iteration_history(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::iteration_history(&state.ctx, &path.id).await {
        Ok(history) => ok(history).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_iteration_history_summary(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::iteration_history_summary(&state.ctx, &path.id).await
    {
        Ok(summary) => ok(summary).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_loop_timeline(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::execution_timeline(&state.ctx, &path.id).await {
        Ok(timeline) => ok(timeline).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_variable_history(
    State(state): State<ApiState>,
    Path(path): Path<IdNamePath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::variable_history(&state.ctx, &path.id, &path.name)
        .await
    {
        Ok(history) => ok(history).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_loop_context_evolution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_loop_registry::context_evolution(&state.ctx, &path.id).await {
        Ok(evolution) => ok(evolution).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_loop_execution_path(
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
            Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
        ))
    }

    async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
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
