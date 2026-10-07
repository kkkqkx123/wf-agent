//! Agent loop run-control handlers: run / stream / pause / resume / cancel.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use wf_api::Message;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;
use crate::sse::{execution_frames, sse_response};

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
    #[schema(value_type = Value)]
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
    #[schema(value_type = Option<Vec<Value>>)]
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
    let config = wf_api::AgentLoopConfig {
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
    let input = wf_api::AgentLoopInput {
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
        Ok(stream) => sse_response(execution_frames(stream)),
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
