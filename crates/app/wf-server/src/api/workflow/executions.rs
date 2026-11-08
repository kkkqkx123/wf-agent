//! Execution domain: workflow execution control (pause/resume/
//! cancel/status/stream), execution list/detail and execution-scoped trigger
//! execution history. Checkpoint / state-view / graph-analysis surfaces live in the
//! sibling modules `checkpoint/checkpoints`, `workflow/execution_state` and
//! `workflow/execution_analysis`.

use std::convert::Infallible;

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use wf_api::WorkflowExecutionListOptions;

use crate::api::agent::loops::{AppendAgentLoopBody, SetAppendModeBody};
use crate::directory::{scope_file_scope, DirectoryScope};
use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, IdSnakeNodePath};
use crate::paged::{fetch_size, ok_page, resolve_page_fields};
use crate::router::ApiState;
use crate::sse::{execution_frames, sse_response};
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── execution trigger + stream ──
        .route("/workflows/{id}/execute", post(handle_execute_workflow))
        .route(
            "/workflows/{id}/execute/stream",
            post(handle_execute_stream),
        )
        .route(
            "/workflows/{id}/execute/background",
            post(handle_execute_background),
        )
        // ── execution list / detail / control ──
        .route("/executions", get(handle_list_executions))
        .route("/executions/count", get(handle_count_executions))
        .route(
            "/executions/{id}",
            get(handle_get_execution).delete(handle_delete_execution),
        )
        .route("/executions/{id}/pause", post(handle_pause))
        .route("/executions/{id}/resume", post(handle_resume))
        .route("/executions/{id}/cancel", post(handle_cancel))
        .route("/executions/{id}/rerun", post(handle_rerun))
        .route("/executions/{id}/status", get(handle_status))
        // ── execution trigger history (ledger view) ──
        .route("/executions/{id}/triggers", get(handle_trigger_history))
        // ── nested agent loops ──
        .route(
            "/executions/{id}/agent-loops",
            get(handle_list_workflow_agent_loops),
        )
        .route(
            "/executions/{id}/agent-loops/{node_id}/append",
            post(handle_append_workflow_agent_loop),
        )
        .route(
            "/executions/{id}/agent-loops/{node_id}/append-mode",
            post(handle_set_workflow_agent_loop_append_mode),
        )
        .route(
            "/executions/{id}/agent-loops/{node_id}/convert-pending",
            post(handle_convert_workflow_agent_loop_pending),
        )
}

#[derive(Deserialize, ToSchema)]
pub struct ExecuteBody {
    pub(crate) input: Option<serde_json::Value>,
    /// Optional per-execution filesystem scope. Missing inherits the parent
    /// execution then the global default.
    #[serde(default)]
    #[schema(value_type = Option<serde_json::Value>)]
    pub(crate) file_scope: Option<wf_types::config::file_scope::ExecutionFileScope>,
    /// Pinned workflow version to execute. Missing runs the stored
    /// definition as it stands; present loads that saved version instead.
    #[serde(default)]
    pub(crate) version: Option<String>,
    /// Runtime budget overrides. Missing leaves the stored default in place.
    #[serde(default)]
    #[schema(minimum = 1)]
    pub(crate) max_steps: Option<u32>,
    #[serde(default)]
    #[schema(minimum = 1)]
    pub(crate) timeout: Option<u64>,
    #[serde(default)]
    #[schema(minimum = 1)]
    pub(crate) max_execution_time: Option<u64>,
    #[serde(default)]
    pub(crate) enable_checkpoints: Option<bool>,
    #[serde(default)]
    #[schema(minimum = 1)]
    pub(crate) node_timeout: Option<u64>,
    #[serde(default)]
    #[schema(minimum = 1)]
    pub(crate) max_pause_duration: Option<u64>,
    #[serde(default)]
    #[schema(minimum = 1)]
    pub(crate) max_navigation_multiplier: Option<u32>,
    #[serde(default)]
    #[schema(minimum = 1)]
    pub(crate) loop_max_iterations_cap: Option<u32>,
    /// Debug marker for direct subworkflow runs. Metadata only; the engine
    /// never reads it for budgets or scheduling.
    #[serde(default)]
    pub(crate) debug: Option<bool>,
}

/// Execution creation inputs derived from the request: engine options
/// plus the stamped owning directory. The stamp is always the request
/// directory itself, never the (possibly narrower) explicit file scope.
fn execution_options(
    body: &ExecuteBody,
    scope: &Option<DirectoryScope>,
) -> Result<
    (
        Option<wf_types::workflow_execution::WorkflowExecutionOptions>,
        Option<String>,
    ),
    wf_api::ApiError,
> {
    // Launching without a directory is rejected: the execution would
    // otherwise run in a workspace the client never named.
    let scope = crate::directory::require_scope(scope)?;
    // The request directory anchors the execution only when the body
    // carries no explicit file scope. An explicit scope is validated
    // against the request directory so it narrows the sandbox instead
    // of escaping it (and the allowlist behind it).
    if let Some(explicit) = body.file_scope.as_ref() {
        crate::directory::check_explicit_file_scope(scope, explicit)?;
    }
    let file_scope = body
        .file_scope
        .clone()
        .or_else(|| Some(scope_file_scope(scope)));
    // Ownership follows the requesting directory, not the sandbox shape:
    // an explicit narrower file scope never moves the stamp.
    let directory = Some(scope.directory.to_string_lossy().into_owned());
    let has_runtime = body.max_steps.is_some()
        || body.timeout.is_some()
        || body.max_execution_time.is_some()
        || body.enable_checkpoints.is_some()
        || body.node_timeout.is_some()
        || body.max_pause_duration.is_some()
        || body.max_navigation_multiplier.is_some()
        || body.loop_max_iterations_cap.is_some()
        || file_scope.is_some();
    if !has_runtime {
        return Ok((None, directory));
    }
    Ok((
        Some(wf_types::workflow_execution::WorkflowExecutionOptions {
            file_scope,
            input: None,
            max_steps: body.max_steps,
            timeout: body.timeout,
            max_execution_time: body.max_execution_time,
            enable_checkpoints: body.enable_checkpoints,
            node_timeout: body.node_timeout,
            max_pause_duration: body.max_pause_duration,
            max_navigation_multiplier: body.max_navigation_multiplier,
            loop_max_iterations_cap: body.loop_max_iterations_cap,
        }),
        directory,
    ))
}

#[derive(Serialize, ToSchema)]
pub struct ExecuteView {
    pub(crate) execution_id: String,
    pub(crate) result: serde_json::Value,
}

#[derive(Serialize)]
pub struct StreamMetadataView {
    execution_id: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/execute",
    tag = "workflow",
    params(IdPath),
    request_body = ExecuteBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::workflow::executions::ExecuteView>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execute_workflow(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
    Json(body): Json<ExecuteBody>,
) -> impl IntoResponse {
    let (options, directory) = match execution_options(&body, &scope) {
        Ok(options) => options,
        Err(e) => return error_response(e),
    };
    let params = wf_api::workflow::workflow_execution::ExecuteWorkflowParams {
        workflow_id: path.id,
        version: body.version,
        input: body.input,
        options,
        debug: body.debug,
        directory,
    };
    match wf_api::workflow::workflow_execution::execute(&state.ctx, params).await {
        Ok(output) => ok(ExecuteView {
            execution_id: output.execution_id.to_string(),
            result: output.result,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/execute/stream",
    tag = "workflow",
    params(IdPath),
    request_body = ExecuteBody,
    responses(
        (status = 200, description = "Server-sent events stream", content_type = "text/event-stream"),
        (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execute_stream(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
    Json(body): Json<ExecuteBody>,
) -> Response {
    let (options, directory) = match execution_options(&body, &scope) {
        Ok(options) => options,
        Err(e) => return error_response(e),
    };
    let params = wf_api::workflow::workflow_execution::ExecuteWorkflowParams {
        workflow_id: path.id,
        version: body.version,
        input: body.input,
        options,
        debug: body.debug,
        directory,
    };
    let (execution_id, stream) =
        match wf_api::workflow::workflow_execution::stream(state.ctx, params).await {
            Ok(pair) => pair,
            Err(e) => return error_response(e),
        };
    let metadata = serde_json::to_string(&StreamMetadataView {
        execution_id: execution_id.to_string(),
    })
    .unwrap_or_else(|_| "{}".to_string());
    let first = Ok::<_, Infallible>(Bytes::from(format!(
        "event: metadata\ndata: {metadata}\n\n"
    )));
    sse_response(
        futures::stream::once(futures::future::ready(first)).chain(execution_frames(stream)),
    )
}

#[derive(Serialize, ToSchema)]
pub(crate) struct BackgroundExecuteView {
    pub(crate) execution_id: String,
    pub(crate) background: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/execute/background",
    tag = "workflow",
    params(IdPath),
    request_body = ExecuteBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::workflow::executions::BackgroundExecuteView>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execute_background(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
    Json(body): Json<ExecuteBody>,
) -> impl IntoResponse {
    let (options, directory) = match execution_options(&body, &scope) {
        Ok(options) => options,
        Err(e) => return error_response(e),
    };
    let params = wf_api::workflow::workflow_execution::ExecuteWorkflowParams {
        workflow_id: path.id,
        version: body.version,
        input: body.input,
        options,
        debug: body.debug,
        directory,
    };
    // `stream` spawns the driver detached from this request and hands back
    // the id immediately. Draining the stream in a server task keeps the
    // driver alive until its terminal event; the client already holds the
    // id and polls status separately.
    match wf_api::workflow::workflow_execution::stream(state.ctx, params).await {
        Ok((execution_id, mut stream)) => {
            let id = execution_id.to_string();
            tokio::spawn(async move {
                while let Some(event) = stream.next().await {
                    if matches!(
                        event,
                        wf_api::infra::stream::ExecutionStreamEvent::Completed { .. }
                            | wf_api::infra::stream::ExecutionStreamEvent::Failed { .. }
                            | wf_api::infra::stream::ExecutionStreamEvent::Interrupted { .. }
                    ) {
                        break;
                    }
                }
            });
            ok(BackgroundExecuteView {
                execution_id: id,
                background: true,
            })
            .into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListExecutionsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    workflow_id: Option<String>,
    status: Option<String>,
    /// Inclusive lower bound on `startedAt` (ms epoch).
    started_from: Option<i64>,
    /// Inclusive upper bound on `startedAt` (ms epoch).
    started_to: Option<i64>,
    /// Sort by start time: `asc` or `desc`. Absent preserves storage order.
    order: Option<String>,
}

/// Filter for the execution count endpoint. Pagination is meaningless
/// for a total, so only the filter dimensions are accepted.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct CountExecutionsQuery {
    workflow_id: Option<String>,
    status: Option<String>,
    started_from: Option<i64>,
    started_to: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ExecutionCountView {
    pub(crate) count: u64,
}

fn parse_order(order: Option<&str>) -> Result<Option<bool>, String> {
    match order {
        None => Ok(None),
        Some(raw) if raw.eq_ignore_ascii_case("desc") => Ok(Some(true)),
        Some(raw) if raw.eq_ignore_ascii_case("asc") => Ok(Some(false)),
        Some(raw) => Err(format!("unknown order: {raw} (expected asc or desc)")),
    }
}

fn parse_status_opt(status: Option<&str>) -> Result<Option<String>, String> {
    match status {
        None => Ok(None),
        Some(raw) => raw
            .parse::<wf_types::ExecutionStatus>()
            .map(|s| Some(s.as_str().to_string()))
            .map_err(|_| format!("unknown status: {raw}")),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions",
    tag = "workflow",
    params(ListExecutionsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_executions(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Query(query): Query<ListExecutionsQuery>,
) -> impl IntoResponse {
    let order_desc = match parse_order(query.order.as_deref()) {
        Ok(order) => order,
        Err(message) => {
            return crate::envelope::err(crate::envelope::InvalidParams::validation(message))
                .into_response()
        }
    };
    let status_filter = match parse_status_opt(query.status.as_deref()) {
        Ok(status) => status,
        Err(message) => {
            return crate::envelope::err(crate::envelope::InvalidParams::validation(message))
                .into_response()
        }
    };
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    let directory = scope
        .as_ref()
        .map(|scope| scope.directory.to_string_lossy().into_owned());
    let options = WorkflowExecutionListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        workflow_id_filter: query.workflow_id,
        status_filter,
        started_from: query.started_from,
        started_to: query.started_to,
        order_desc,
        directory_filter: directory,
    };
    match wf_api::workflow::list_executions(&state.ctx, Some(options)).await {
        Ok(executions) => {
            let visible: Vec<_> = executions
                .into_iter()
                .filter(|execution| {
                    crate::directory::check_owner(&scope, execution.directory.as_deref())
                })
                .collect();
            ok_page(visible, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/count",
    tag = "workflow",
    params(CountExecutionsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::workflow::executions::ExecutionCountView>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_count_executions(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Query(query): Query<CountExecutionsQuery>,
) -> impl IntoResponse {
    let status_filter = match parse_status_opt(query.status.as_deref()) {
        Ok(status) => status,
        Err(message) => {
            return crate::envelope::err(crate::envelope::InvalidParams::validation(message))
                .into_response()
        }
    };
    let directory = scope
        .as_ref()
        .map(|scope| scope.directory.to_string_lossy().into_owned());
    let options = WorkflowExecutionListOptions {
        offset: None,
        limit: None,
        workflow_id_filter: query.workflow_id,
        status_filter,
        started_from: query.started_from,
        started_to: query.started_to,
        order_desc: None,
        directory_filter: directory,
    };
    // Directory-less requests cannot express "missing directory" in the
    // storage filter, so they count the unowned subset in Rust instead of
    // seeing owned runs.
    if scope.is_none() {
        return match wf_api::workflow::list_executions(&state.ctx, Some(options)).await {
            Ok(executions) => {
                let count = executions
                    .iter()
                    .filter(|execution| execution.directory.is_none())
                    .count() as u64;
                ok(ExecutionCountView { count }).into_response()
            }
            Err(e) => error_response(e),
        };
    }
    match wf_api::workflow::execution::count_executions(&state.ctx, Some(options)).await {
        Ok(count) => ok(ExecutionCountView { count }).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_execution(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::workflow::get_execution(&state.ctx, &path.id).await {
        Ok(execution) => ok(execution).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/executions/{id}",
    tag = "workflow",
    params(IdPath, DeleteExecutionQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_execution(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
    Query(query): Query<DeleteExecutionQuery>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    // The default is a shallow delete: only the workflow execution
    // record goes away. `full` additionally removes the linked agent
    // execution and agent loop records.
    let deleted = if query.full.unwrap_or(false) {
        wf_api::workflow::delete_execution_full(&state.ctx, &path.id).await
    } else {
        wf_api::workflow::delete_execution(&state.ctx, &path.id).await
    };
    match deleted {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct DeleteExecutionQuery {
    /// Also delete linked agent execution and agent loop records.
    full: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/pause",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_pause(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::workflow::workflow_execution::pause(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/resume",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::workflow::executions::ExecuteView>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_resume(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::workflow::workflow_execution::resume(&state.ctx, &path.id).await {
        Ok(output) => ok(ExecuteView {
            execution_id: output.execution_id.to_string(),
            result: output.result,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/cancel",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_cancel(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::workflow::workflow_execution::cancel(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

/// Server-side rerun: the old execution supplies workflow id, pinned
/// version and input, and the new run's record points back at it.
#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/rerun",
    operation_id = "post_executions_id_rerun",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Rerun started from the recorded execution", body = crate::envelope::ApiEnvelope<crate::api::workflow::executions::ExecuteView>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rerun(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::resolve_execution(&state.ctx, &path.id).await {
        Ok(wf_api::ExecutionDomain::Workflow) => {}
        Ok(wf_api::ExecutionDomain::AgentLoop) => {
            return error_response(wf_api::ApiError::Validation(
                "rerun is only supported for workflow executions".to_string(),
            ));
        }
        Err(e) => return error_response(e),
    }
    match wf_api::workflow::workflow_execution::rerun(&state.ctx, &path.id).await {
        Ok(output) => ok(ExecuteView {
            execution_id: output.execution_id.to_string(),
            result: output.result,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/status",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_status(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::workflow::workflow_execution::status(&state.ctx, &path.id).await {
        Ok(status) => ok(status).into_response(),
        Err(e) => error_response(e),
    }
}

// ── execution triggers ────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct TriggerHistoryQuery {
    trigger_name: Option<String>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/triggers",
    operation_id = "get_executions_id_triggers",
    tag = "workflow",
    params(IdPath, TriggerHistoryQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_trigger_history(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
    Query(query): Query<TriggerHistoryQuery>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::trigger::execution::execution_history(
        &state.ctx.storage,
        &path.id,
        query.trigger_name.as_deref(),
    )
    .await
    {
        Ok(history) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
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
    path = "/api/v1/executions/{id}/agent-loops",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_workflow_agent_loops(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::workflow::list_agent_loops(&state.ctx, &path.id).await {
        Ok(loops) => ok(loops).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/agent-loops/{node_id}/append",
    tag = "workflow",
    params(IdSnakeNodePath),
    request_body = AppendAgentLoopBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_append_workflow_agent_loop(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdSnakeNodePath>,
    Json(body): Json<AppendAgentLoopBody>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::workflow::append_to_agent_loop(
        &state.ctx,
        &path.id,
        &path.node_id,
        body.text,
        body.as_mode.as_deref(),
        body.source.as_deref(),
    )
    .await
    {
        Ok(()) => ok(serde_json::json!({})).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/agent-loops/{node_id}/append-mode",
    tag = "workflow",
    params(IdSnakeNodePath),
    request_body = SetAppendModeBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_set_workflow_agent_loop_append_mode(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdSnakeNodePath>,
    Json(body): Json<SetAppendModeBody>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::workflow::set_agent_loop_append_mode(
        &state.ctx,
        &path.id,
        &path.node_id,
        &body.mode,
    )
    .await
    {
        Ok(()) => ok(serde_json::json!({})).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/agent-loops/{node_id}/convert-pending",
    tag = "workflow",
    params(IdSnakeNodePath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_convert_workflow_agent_loop_pending(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<DirectoryScope>>,
    Path(path): Path<IdSnakeNodePath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_workflow_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::workflow::convert_pending_to_steering(&state.ctx, &path.id, &path.node_id).await {
        Ok(()) => ok(serde_json::json!({})).into_response(),
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
        ))
    }

    async fn send(ctx: Arc<ApiContext>, method: &str, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn json_body(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn executions_are_queryable() {
        let ctx = make_ctx();

        let executions = send(ctx.clone(), "GET", "/api/v1/executions").await;
        assert_eq!(executions.status(), StatusCode::OK);

        let missing = send(ctx.clone(), "GET", "/api/v1/executions/nope").await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let body = json_body(missing).await;
        assert_eq!(body["success"], false);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn execution_control_maps_not_found() {
        let ctx = make_ctx();
        // Control / command operations require a live entity and map to NotFound.
        for (method, uri) in [
            ("POST", "/api/v1/executions/missing/pause"),
            ("POST", "/api/v1/executions/missing/resume"),
            ("POST", "/api/v1/executions/missing/cancel"),
            ("GET", "/api/v1/executions/missing/status"),
        ] {
            let response = send(ctx.clone(), method, uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
        }
        // Read / analysis views share the existence gate: unknown ids are
        // NotFound instead of degrading to empty results.
        for (method, uri) in [
            ("GET", "/api/v1/executions/missing/state"),
            ("GET", "/api/v1/executions/missing/variables"),
            ("GET", "/api/v1/executions/missing/transitions"),
            ("GET", "/api/v1/executions/missing/context"),
            ("GET", "/api/v1/executions/missing/call-stack"),
            ("GET", "/api/v1/executions/missing/memory"),
            ("GET", "/api/v1/executions/missing/variable-snapshots"),
            ("GET", "/api/v1/executions/missing/context-evolution"),
            ("GET", "/api/v1/executions/missing/state-analysis"),
            ("GET", "/api/v1/executions/missing/nodes"),
            ("GET", "/api/v1/executions/missing/nodes/node-1"),
            ("GET", "/api/v1/executions/missing/iterations"),
            ("GET", "/api/v1/executions/missing/path"),
            ("GET", "/api/v1/executions/missing/optimizations"),
            ("GET", "/api/v1/executions/missing/node-stats"),
            ("GET", "/api/v1/executions/missing/failed-nodes"),
        ] {
            let response = send(ctx.clone(), method, uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
        }
        // Graph reconstruction over an unknown execution is NotFound.
        let graph = send(ctx.clone(), "GET", "/api/v1/executions/missing/graph").await;
        assert_eq!(graph.status(), StatusCode::NOT_FOUND);

        // Analysis / state views over an unknown execution are NotFound.
        for uri in [
            "/api/v1/executions/missing/context-transitions",
            "/api/v1/executions/missing/context-snapshots",
            "/api/v1/executions/missing/nodes/by-type/llm",
            "/api/v1/executions/missing/llm-reasoning-path/node-1",
            "/api/v1/executions/missing/nodes/node-1/input-context",
            "/api/v1/executions/missing/nodes/node-1/transitions",
            "/api/v1/executions/missing/graph/reachability",
            "/api/v1/executions/missing/analysis/paths/enumerate",
            "/api/v1/executions/missing/analysis/decision-points",
            "/api/v1/executions/missing/analysis/slow-nodes",
            "/api/v1/executions/missing/analysis/efficiency",
            "/api/v1/executions/missing/analysis/alternatives",
            "/api/v1/executions/missing/analysis/probabilities",
        ] {
            let response = send(ctx.clone(), "GET", uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
        }
    }
    #[tokio::test]
    async fn checkpoint_routes_are_queryable() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/checkpoints",
            "/api/v1/checkpoints/entity/some-entity",
            "/api/v1/checkpoints/entity/some-entity/latest",
            "/api/v1/checkpoints/entity/some-entity/metadata",
            "/api/v1/checkpoints/entities?entity_ids=a,b",
            "/api/v1/checkpoints/time-range?workflowId=wf-1&start=0&end=999",
        ] {
            let response = send(ctx.clone(), "GET", uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }

        let chain = send(ctx, "GET", "/api/v1/executions/exec-1/checkpoints/chain").await;
        assert_eq!(chain.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn slow_nodes_rejects_out_of_range_percentile() {
        let ctx = make_ctx();
        // The existence gate runs before parameter validation, so the
        // percentile case needs a stored execution to reach the handler.
        wf_api::workflow::save_execution(
            &ctx,
            &wf_types::WorkflowExecution {
                id: "exec-percentile".into(),
                workflow_id: "wf-percentile".into(),
                workflow_version: None,
                status: wf_types::ExecutionStatus::Created,
                current_node_id: None,
                rerun_of: None,
                graph: None,
                variables: None,
                input: None,
                output: None,
                node_results: None,
                errors: None,
                started_at: wf_common::now(),
                completed_at: None,
                error: None,
                execution_type: None,
                fork_join_context: None,
                hierarchy: None,
                debug: None,
                directory: None,
            },
        )
        .await
        .unwrap();
        let response = send(
            ctx,
            "GET",
            "/api/v1/executions/exec-percentile/analysis/slow-nodes?percentile=150",
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn state_records_endpoints_map_unknown_to_not_found() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/executions/e-1/state-records",
            "/api/v1/executions/e-1/state-records/iterations/3",
            "/api/v1/executions/e-1/state-records/snapshots/1234",
            "/api/v1/executions/e-1/state-records/variables/x/history",
            "/api/v1/executions/e-1/state-records/most-changed",
            "/api/v1/executions/e-1/state-records/mutation-count",
            "/api/v1/executions/e-1/state-records/call-stack",
            "/api/v1/executions/e-1/state-records/memory",
            "/api/v1/executions/e-1/state-records/memory/peak",
        ] {
            let response = send(ctx.clone(), "GET", uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
        }

        let cleared = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/executions/e-1/state-records")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cleared.status(), StatusCode::OK);
    }
}
