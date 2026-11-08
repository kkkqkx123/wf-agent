//! Agent loop run-control handlers: run / stream / pause / resume / cancel.

use std::collections::HashMap;

use axum::extract::{Extension, Path, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use wf_api::Message;

use crate::directory::{scope_file_scope, DirectoryScope};
use crate::envelope::{error_response, ok, session_error_response};
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
    /// Optional per-execution filesystem scope. Missing inherits the parent
    /// execution then the global default; explicit values override per field.
    #[serde(default)]
    #[schema(value_type = Option<Value>)]
    file_scope: Option<wf_types::config::file_scope::ExecutionFileScope>,
    /// Temporary execution mode of this run (`restricted` for a draft's
    /// fenced first turns, `ephemeral` for a short read-only Q&A without
    /// checkpoints). Absent means a full agent run. Unknown values are
    /// rejected; the engine forces the exposure server-side.
    #[serde(default)]
    temporary_mode: Option<String>,
    #[serde(default)]
    context: HashMap<String, Value>,
    #[schema(value_type = Option<Vec<Value>>)]
    conversation: Option<Vec<Message>>,
    /// Optional shape contract for `context`. Unknown keys stay accepted;
    /// declared keys must match their shape.
    #[serde(default)]
    #[schema(value_type = Option<Vec<Value>>)]
    context_schema: Option<Vec<wf_types::TemplateVariableDefinition>>,
    /// Structured image payload of the turn: inline base64 entries (with
    /// a media type), remote addresses or `data:` URIs. Absent stays a
    /// pure-text turn. The server validates media types, sizes and counts.
    #[serde(default)]
    #[schema(value_type = Option<Vec<Value>>)]
    images: Option<Vec<wf_types::message::UserImageInput>>,
    /// Structured text attachments of the turn. The server validates
    /// counts, sizes and truncation markers, then re-inlines them with
    /// attributes for storage so history stays a single user bubble.
    #[serde(default)]
    #[schema(value_type = Option<Vec<Value>>)]
    attachments: Option<Vec<wf_types::message::UserTextAttachment>>,
}

pub(crate) async fn params_from_body(
    state: &ApiState,
    scope: &Option<DirectoryScope>,
    body: RunAgentLoopBody,
) -> Result<wf_api::agent::agent_execution::RunAgentLoopParams, wf_api::ApiError> {
    // Launching without a directory is rejected: the run would otherwise
    // touch a workspace the client never named.
    let scope = crate::directory::require_scope(scope)?;
    // An explicit file scope narrows the sandbox; it must not escape the
    // request directory (and the allowlist behind it).
    if let Some(explicit) = body.file_scope.as_ref() {
        crate::directory::check_explicit_file_scope(scope, explicit)?;
    }
    // Server is the single washing point: raw client text is cleaned here
    // before the blank check so ANSI-only messages are rejected as empty.
    let message = wf_types::message::sanitize_user_text(&body.message);
    let images = body.images.unwrap_or_default();
    if let Err(reason) = wf_types::message::validate_user_images(&images) {
        return Err(wf_api::ApiError::Validation(format!(
            "agent images are invalid: {reason}"
        )));
    }
    let attachments = body.attachments.unwrap_or_default();
    if let Err(reason) = wf_types::message::validate_user_attachments(&attachments) {
        return Err(wf_api::ApiError::Validation(format!(
            "agent attachments are invalid: {reason}"
        )));
    }
    // Structured attachments ride separately so truncation stays explicit;
    // storage keeps a single user bubble by re-inlining with attributes.
    let message = wf_types::message::format_attachments_for_storage(&message, &attachments);
    // Rule A1: an empty message is a hard error on every entry path, unless
    // images or attachments ride along; the CLI/TUI forms and the web panel
    // all converge here.
    if wf_types::message::is_blank_user_text(&message)
        && images.is_empty()
        && attachments.is_empty()
    {
        return Err(wf_api::ApiError::Validation(
            "agent message must not be empty".to_string(),
        ));
    }
    // Rule A2: an empty model means the server default template. The CLI
    // encodes `None` as the "default" sentinel on the wire; both spellings
    // normalize here so templates stay the single source of the default.
    let model = if body.model.trim().is_empty() || body.model.trim() == wf_api::DEFAULT_MODEL {
        wf_api::DEFAULT_MODEL.to_string()
    } else {
        body.model
    };
    // Empty ids fall back to the built-in main agent, matching the CLI
    // composition path (`build_agent_loop_config` defaults `None` the same
    // way); the template resolver still rejects unregistered built-ins.
    let agent_id = if body.agent_id.trim().is_empty() {
        wf_api::DEFAULT_AGENT.to_string()
    } else {
        body.agent_id
    };
    // The temporary mode is a strict wire enum: unknown values must not
    // silently become full runs, and full runs must not silently become
    // fenced ones.
    let temporary_mode = match body.temporary_mode {
        None => None,
        Some(raw) => Some(
            wf_types::agent_execution::TemporaryMode::parse(&raw).ok_or_else(|| {
                wf_api::ApiError::Validation(format!(
                    "unknown temporary_mode: {raw} (expected restricted or ephemeral)"
                ))
            })?,
        ),
    };
    let config = wf_api::AgentLoopConfig {
        // The request directory anchors the run only when the body
        // carries no explicit file scope, so explicit values always win.
        file_scope: body.file_scope.or_else(|| Some(scope_file_scope(scope))),
        agent_id: wf_api::Id::from(agent_id),
        model,
        max_iterations: body.max_iterations,
        max_execution_time: body.max_execution_time,
        hooks: body
            .hooks
            .unwrap_or_default()
            .into_iter()
            .filter_map(|h| serde_json::from_value(h).ok())
            .collect(),
        available_tool_names: body.available_tool_names,
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
        context_schema: body.context_schema,
        context_strict: None,
        temporary_mode,
        checkpoint_message_interval: body.checkpoint_message_interval.filter(|n| *n > 0),
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
    let (mention_messages, mention_images, mention_receipts) =
        wf_api::agent::composition::resolve_mentions(&state.ctx, &message).await;
    let input = wf_api::AgentLoopInput {
        message,
        context: body.context,
        conversation: body.conversation.unwrap_or_default(),
        images,
    };
    let mut params = wf_api::agent::composition::resolve_run_params(
        &env,
        wf_api::agent::agent_execution::RunAgentLoopParams::new(config, input),
    )?;

    for message in mention_messages {
        // Supplemental context from inline mentions.
        params.input.conversation.push(message);
    }
    // Image references resolve beside the text mentions and ride the
    // structured payload, never the message text.
    params.input.images.extend(mention_images);
    // Per-reference receipt so clients can show which mentions actually
    // resolved: metadata carries the structured JSON, the text stays a
    // human-readable mirror for the model and transcripts.
    if !mention_receipts.is_empty() {
        let lines: Vec<String> = mention_receipts
            .iter()
            .map(|receipt| format!("@{} → {}", receipt.reference, receipt.outcome))
            .collect();
        let mut receipt_message =
            wf_api::dynamic_context_message(format!("mention receipts: {}", lines.join("; ")));
        if let Ok(json) = serde_json::to_string(&mention_receipts) {
            if let Some(metadata) = receipt_message.metadata.as_mut() {
                metadata.insert(
                    "mention_receipts".to_string(),
                    serde_json::Value::String(json),
                );
            }
        }
        params.input.conversation.push(receipt_message);
    }
    // Ownership follows the requesting directory, not the sandbox shape:
    // an explicit narrower file scope never moves the stamp.
    params.directory = Some(scope.directory.to_string_lossy().into_owned());
    Ok(params)
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
    Extension(scope): Extension<Option<DirectoryScope>>,
    Json(body): Json<RunAgentLoopBody>,
) -> impl IntoResponse {
    match wf_api::agent::agent_execution::run(
        &state.ctx,
        match params_from_body(&state, &scope, body).await {
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
    Extension(scope): Extension<Option<DirectoryScope>>,
    Json(body): Json<RunAgentLoopBody>,
) -> Response {
    match wf_api::agent::agent_execution::stream(
        state.ctx.clone(),
        match params_from_body(&state, &scope, body).await {
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
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_agent_owner(&state.ctx, &scope, &path.id).await {
        return session_error_response(e);
    }

    match wf_api::agent::agent_execution::pause(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => session_error_response(e),
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
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_agent_owner(&state.ctx, &scope, &path.id).await {
        return session_error_response(e);
    }

    match wf_api::agent::agent_execution::resume(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => session_error_response(e),
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
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_agent_owner(&state.ctx, &scope, &path.id).await {
        return session_error_response(e);
    }

    match wf_api::agent::agent_execution::cancel(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => session_error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct AppendAgentLoopBody {
    pub(crate) text: String,
    /// Runtime append mode. Accepts both the documented `as` field and the
    /// historical `as_mode` field.
    #[serde(alias = "as", default)]
    pub(crate) as_mode: Option<String>,
    /// Calling channel for the audit trail (`user`/`cli`/`tui`/`web`/
    /// `workflow-console`); absent values default to `api`.
    #[serde(default)]
    pub(crate) source: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetAppendModeBody {
    pub(crate) mode: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/append",
    tag = "agent",
    params(IdPath),
    request_body = AppendAgentLoopBody,
    responses(
        (status = 200, description = "Input appended", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid request", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_append_loop(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
    Json(body): Json<AppendAgentLoopBody>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_agent_owner(&state.ctx, &scope, &path.id).await {
        return session_error_response(e);
    }

    match wf_api::agent::agent_execution::append(
        &state.ctx,
        &path.id,
        body.text,
        body.as_mode.as_deref(),
        body.source.as_deref(),
        None,
    )
    .await
    {
        Ok(()) => ok(()).into_response(),
        Err(e) => session_error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/append-mode",
    tag = "agent",
    params(IdPath),
    request_body = SetAppendModeBody,
    responses(
        (status = 200, description = "Append mode updated", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid request", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_set_append_mode_loop(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
    Json(body): Json<SetAppendModeBody>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_agent_owner(&state.ctx, &scope, &path.id).await {
        return session_error_response(e);
    }

    match wf_api::agent::agent_execution::set_append_mode(&state.ctx, &path.id, &body.mode).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => session_error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/convert-pending",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Pending inputs converted", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_convert_pending_to_steering_loop(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_agent_owner(&state.ctx, &scope, &path.id).await {
        return session_error_response(e);
    }

    match wf_api::agent::agent_execution::convert_pending_to_steering(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => session_error_response(e),
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct ForkLoopBody {
    #[serde(default)]
    checkpoint_id: Option<String>,
    #[serde(flatten)]
    run: RunAgentLoopBody,
}

/// Fork a live loop through a checkpoint branch without copying product
/// storage. Missing checkpoint selects the latest restorable checkpoint;
/// a foreign checkpoint maps to a conflict so senders retry rather than
/// treating the failure as a parameter error.
#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/fork",
    tag = "agent",
    params(IdPath),
    request_body = ForkLoopBody,
    responses(
        (status = 200, description = "Forked loop output", body = crate::envelope::ApiEnvelope<crate::api::agent::loops::AgentRunView>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Checkpoint belongs to another session", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_fork_loop(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
    Json(body): Json<ForkLoopBody>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_agent_owner(&state.ctx, &scope, &path.id).await {
        return session_error_response(e);
    }
    let checkpoint_id = match body.checkpoint_id {
        Some(id) => id,
        None => match latest_restorable_checkpoint(&state.ctx, &path.id).await {
            Ok(id) => id,
            Err(e) => return session_error_response(e),
        },
    };
    let run_params = match params_from_body(&state, &scope, body.run).await {
        Ok(params) => params,
        Err(e) => return session_error_response(e),
    };
    match wf_api::agent::agent_execution::resume_from_checkpoint(
        &state.ctx,
        &path.id,
        &checkpoint_id,
        run_params,
        false,
    )
    .await
    {
        Ok(output) => ok(AgentRunView {
            agent_loop_id: output.agent_loop_id.to_string(),
            result: output.result,
            iterations: output.iterations,
        })
        .into_response(),
        Err(e) => inbox_conflict_response(e),
    }
}

/// Transfer a loop by branching its conversation to a fresh handle.
/// Storage stays shared through the checkpoint lineage; the source record
/// is left untouched so the operation stays idempotent.
#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/transfer",
    tag = "agent",
    params(IdPath),
    request_body = ForkLoopBody,
    responses(
        (status = 200, description = "Transferred loop output", body = crate::envelope::ApiEnvelope<crate::api::agent::loops::AgentRunView>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Checkpoint belongs to another session", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_transfer_loop(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
    Json(body): Json<ForkLoopBody>,
) -> impl IntoResponse {
    handle_fork_loop(
        State(state),
        Extension(scope),
        Path(path),
        Json(ForkLoopBody {
            checkpoint_id: body.checkpoint_id,
            run: body.run,
        }),
    )
    .await
}

/// Share a loop as a read-only view without duplicating storage.
#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/share",
    tag = "agent",
    params(IdPath),
    responses(
        (status = 200, description = "Shared loop view", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_share_loop(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_agent_owner(&state.ctx, &scope, &path.id).await {
        return session_error_response(e);
    }
    match wf_api::agent::agent_loop_registry::summary(&state.ctx, &path.id).await {
        Ok(Some(summary)) => {
            ok(serde_json::json!({ "shared": true, "loop": summary })).into_response()
        }
        Ok(None) => crate::envelope::session_not_found(format!("session [{}] not found", path.id))
            .into_response(),
        Err(e) => session_error_response(e),
    }
}

/// Import a transcript using the same normalization as live runs so old
/// exports replay without format drift.
#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/import",
    tag = "agent",
    request_body = RunAgentLoopBody,
    responses(
        (status = 200, description = "Imported loop output", body = crate::envelope::ApiEnvelope<crate::api::agent::loops::AgentRunView>),
        (status = 400, description = "Invalid transcript", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import_loop(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Json(body): Json<RunAgentLoopBody>,
) -> impl IntoResponse {
    match params_from_body(&state, &scope, body).await {
        Ok(params) => match wf_api::agent::agent_execution::run(&state.ctx, params).await {
            Ok(output) => ok(AgentRunView {
                agent_loop_id: output.agent_loop_id.to_string(),
                result: output.result,
                iterations: output.iterations,
            })
            .into_response(),
            Err(e) => session_error_response(e),
        },
        Err(e) => session_error_response(e),
    }
}

async fn latest_restorable_checkpoint(
    ctx: &wf_api::ApiContext,
    agent_loop_id: &str,
) -> Result<String, wf_api::ApiError> {
    wf_api::agent::agent_checkpoint::latest_restorable_checkpoint_id(ctx, agent_loop_id).await
}

/// A checkpoint that exists under another session is an inbox conflict:
/// report a conflict so the sender retries against the owning session
/// instead of treating the failure as a malformed request.
fn inbox_conflict_response(e: wf_api::ApiError) -> axum::response::Response {
    match &e {
        wf_api::ApiError::Validation(message) if message.contains("does not belong") => {
            crate::envelope::session_error_response(wf_api::ApiError::Conflict(message.clone()))
        }
        _ => session_error_response(e),
    }
}
