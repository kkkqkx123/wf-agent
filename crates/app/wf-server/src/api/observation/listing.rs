//! Unified execution listing: one time-ordered view over workflow runs,
//! agent loop runs and chat sessions. Per-engine lists stay where they are;
//! this route merges the sides for callers that do not care which executor
//! owns a run, and the detail route below answers for one id across engines.

use axum::extract::{Extension, Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use wf_api::execution_listing::{UnifiedExecutionFilter, UnifiedExecutionSummary};

use crate::envelope::{err, InvalidParams};
use crate::extract::IdPath;
use crate::paged::{ok_cursor_page, resolve_cursor_page};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/unified-executions", get(handle_unified_executions))
        .route(
            "/unified-executions/count",
            get(handle_unified_executions_count),
        )
        .route("/unified-executions/{id}", get(handle_unified_execution))
        .route("/unified-executions/{id}/pause", post(handle_unified_pause))
        .route(
            "/unified-executions/{id}/resume",
            post(handle_unified_resume),
        )
        .route(
            "/unified-executions/{id}/cancel",
            post(handle_unified_cancel),
        )
        .route("/unified-executions/{id}/rerun", post(handle_unified_rerun))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct UnifiedExecutionsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Opaque cursor from a previous page
    cursor: Option<String>,
    /// Filter by status (running, paused, completed, failed, ...)
    status: Option<String>,
    /// Filter by executor (`workflow`, `agent` or `chat`)
    execution_type: Option<String>,
    /// Filter by definition (workflow id, or agent definition id)
    definition_id: Option<String>,
    /// Inclusive lower bound on start time (ms epoch)
    started_from: Option<i64>,
    /// Inclusive upper bound on start time (ms epoch)
    started_to: Option<i64>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct UnifiedExecutionsCountQuery {
    /// Filter by status (running, paused, completed, failed, ...)
    status: Option<String>,
    /// Filter by executor (`workflow`, `agent` or `chat`)
    execution_type: Option<String>,
    /// Filter by definition (workflow id, or agent definition id)
    definition_id: Option<String>,
    /// Inclusive lower bound on start time (ms epoch)
    started_from: Option<i64>,
    /// Inclusive upper bound on start time (ms epoch)
    started_to: Option<i64>,
}

/// One run in the unified listing, regardless of owning engine.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UnifiedExecutionDoc {
    execution_id: String,
    #[schema(value_type = String)]
    execution_type: wf_types::execution::ExecutionType,
    #[schema(value_type = String)]
    status: wf_types::ExecutionStatus,
    start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    definition_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rerun_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    debug: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl From<UnifiedExecutionSummary> for UnifiedExecutionDoc {
    fn from(summary: UnifiedExecutionSummary) -> Self {
        Self {
            execution_id: summary.execution_id,
            execution_type: summary.execution_type,
            status: summary.status,
            start_time: summary.start_time,
            end_time: summary.end_time,
            definition_id: summary.definition_id,
            version: summary.version,
            rerun_of: summary.rerun_of,
            debug: summary.debug,
            parent_execution_id: summary.parent_execution_id,
            error: summary.error,
        }
    }
}

fn parse_filter(
    status: Option<&str>,
    execution_type: Option<&str>,
    definition_id: Option<String>,
    started_from: Option<i64>,
    started_to: Option<i64>,
    directory: Option<String>,
) -> Result<UnifiedExecutionFilter, String> {
    let status = match status {
        None => None,
        Some(raw) => Some(
            raw.parse::<wf_types::ExecutionStatus>()
                .map_err(|_| format!("unknown status: {raw}"))?,
        ),
    };
    let execution_type = match execution_type {
        None => None,
        Some(raw) => Some(
            wf_types::execution::ExecutionType::parse(raw)
                .ok_or_else(|| format!("unknown execution_type: {raw}"))?,
        ),
    };
    Ok(UnifiedExecutionFilter {
        status,
        execution_type,
        definition_id,
        started_from,
        started_to,
        directory,
    })
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UnifiedExecutionCountView {
    pub(crate) count: usize,
}

#[utoipa::path(
    get,
    path = "/api/v1/unified-executions",
    tag = "observation",
    params(UnifiedExecutionsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::CursorPageView<crate::api::observation::listing::UnifiedExecutionDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unified_executions(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Query(query): Query<UnifiedExecutionsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = match resolve_cursor_page(query.limit, query.cursor.as_deref()) {
        Ok(page) => page,
        Err(message) => return err(InvalidParams::validation(message)).into_response(),
    };
    let filter = match parse_filter(
        query.status.as_deref(),
        query.execution_type.as_deref(),
        query.definition_id.clone(),
        query.started_from,
        query.started_to,
        scope
            .as_ref()
            .map(|scope| scope.directory.to_string_lossy().into_owned()),
    ) {
        Ok(filter) => filter,
        Err(message) => return err(InvalidParams::validation(message)).into_response(),
    };
    // Ask for one extra item past the page: `from_window` turns the extra
    // item into `has_more` plus the following cursor and drops it.
    let fetch = (limit as usize).saturating_add(1);
    match wf_api::execution_listing::list_unified(&state.ctx, Some(&filter), fetch, offset as usize)
        .await
    {
        Ok((summaries, _)) => {
            let window = summaries
                .into_iter()
                .map(UnifiedExecutionDoc::from)
                .collect::<Vec<_>>();
            ok_cursor_page(window, limit, offset).into_response()
        }
        Err(e) => crate::envelope::error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/unified-executions/count",
    tag = "observation",
    params(UnifiedExecutionsCountQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::observation::listing::UnifiedExecutionCountView>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unified_executions_count(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Query(query): Query<UnifiedExecutionsCountQuery>,
) -> impl IntoResponse {
    let filter = match parse_filter(
        query.status.as_deref(),
        query.execution_type.as_deref(),
        query.definition_id.clone(),
        query.started_from,
        query.started_to,
        scope
            .as_ref()
            .map(|scope| scope.directory.to_string_lossy().into_owned()),
    ) {
        Ok(filter) => filter,
        Err(message) => return err(InvalidParams::validation(message)).into_response(),
    };
    match wf_api::execution_listing::count_unified(&state.ctx, Some(&filter)).await {
        Ok(count) => crate::envelope::ok(UnifiedExecutionCountView { count }).into_response(),
        Err(e) => crate::envelope::error_response(e),
    }
}

/// One execution across engines, tagged with its real executor. The chat
/// transcript is not embedded: a chat detail carries the session id as
/// `execution_id`, and the caller reads its messages through the existing
/// conversation endpoint.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UnifiedExecutionDetailDoc {
    execution_id: String,
    #[schema(value_type = String)]
    execution_type: wf_types::execution::ExecutionType,
    #[schema(value_type = String)]
    status: wf_types::ExecutionStatus,
    start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    definition_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rerun_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    record: serde_json::Value,
}

impl From<wf_api::execution_listing::UnifiedExecutionDetail> for UnifiedExecutionDetailDoc {
    fn from(detail: wf_api::execution_listing::UnifiedExecutionDetail) -> Self {
        Self {
            execution_id: detail.execution_id,
            execution_type: detail.execution_type,
            status: detail.status,
            start_time: detail.start_time,
            end_time: detail.end_time,
            definition_id: detail.definition_id,
            version: detail.version,
            rerun_of: detail.rerun_of,
            parent_execution_id: detail.parent_execution_id,
            error: detail.error,
            record: detail.record,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/unified-executions/{id}",
    tag = "observation",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::observation::listing::UnifiedExecutionDetailDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unified_execution(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return crate::envelope::error_response(e);
    }
    match wf_api::execution_listing::get_unified_detail(&state.ctx, &path.id).await {
        Ok(detail) => crate::envelope::ok(UnifiedExecutionDetailDoc::from(detail)).into_response(),
        Err(e) => crate::envelope::error_response(e),
    }
}

/// Domain tag carried by unified control responses.
fn execution_type_of(domain: wf_api::ExecutionDomain) -> wf_types::execution::ExecutionType {
    match domain {
        wf_api::ExecutionDomain::Workflow => wf_types::execution::ExecutionType::Workflow,
        wf_api::ExecutionDomain::AgentLoop => wf_types::execution::ExecutionType::Agent,
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UnifiedControlView {
    pub(crate) execution_id: String,
    #[schema(value_type = String)]
    pub(crate) execution_type: wf_types::execution::ExecutionType,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UnifiedResumeView {
    pub(crate) execution_id: String,
    #[schema(value_type = String)]
    pub(crate) execution_type: wf_types::execution::ExecutionType,
    /// Workflow resume drives the run to completion and returns its
    /// result; agent resume only signals the loop, so the field is absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<serde_json::Value>,
}

/// Pause one execution without caring which engine owns it. The id is
/// resolved across workflow and agent-loop records first, so unknown ids
/// are `NotFound` instead of silent success.
#[utoipa::path(
    post,
    path = "/api/v1/unified-executions/{id}/pause",
    tag = "observation",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::observation::listing::UnifiedControlView>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unified_pause(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return crate::envelope::error_response(e);
    }
    let domain = match wf_api::resolve_execution(&state.ctx, &path.id).await {
        Ok(domain) => domain,
        Err(e) => return crate::envelope::error_response(e),
    };
    let result = match domain {
        wf_api::ExecutionDomain::Workflow => {
            wf_api::workflow::workflow_execution::pause(&state.ctx, &path.id).await
        }
        wf_api::ExecutionDomain::AgentLoop => {
            wf_api::agent::agent_execution::pause(&state.ctx, &path.id).await
        }
    };
    match result {
        Ok(()) => crate::envelope::ok(UnifiedControlView {
            execution_id: path.id,
            execution_type: execution_type_of(domain),
        })
        .into_response(),
        Err(e) => crate::envelope::error_response(e),
    }
}

/// Cancel one execution without caring which engine owns it.
#[utoipa::path(
    post,
    path = "/api/v1/unified-executions/{id}/cancel",
    tag = "observation",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::observation::listing::UnifiedControlView>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unified_cancel(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return crate::envelope::error_response(e);
    }
    let domain = match wf_api::resolve_execution(&state.ctx, &path.id).await {
        Ok(domain) => domain,
        Err(e) => return crate::envelope::error_response(e),
    };
    let result = match domain {
        wf_api::ExecutionDomain::Workflow => {
            wf_api::workflow::workflow_execution::cancel(&state.ctx, &path.id).await
        }
        wf_api::ExecutionDomain::AgentLoop => {
            wf_api::agent::agent_execution::cancel(&state.ctx, &path.id).await
        }
    };
    match result {
        Ok(()) => crate::envelope::ok(UnifiedControlView {
            execution_id: path.id,
            execution_type: execution_type_of(domain),
        })
        .into_response(),
        Err(e) => crate::envelope::error_response(e),
    }
}

/// Resume one execution without caring which engine owns it. Workflow
/// resume runs to completion and returns the result; agent resume only
/// signals the live loop.
#[utoipa::path(
    post,
    path = "/api/v1/unified-executions/{id}/resume",
    tag = "observation",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::observation::listing::UnifiedResumeView>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unified_resume(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return crate::envelope::error_response(e);
    }
    let domain = match wf_api::resolve_execution(&state.ctx, &path.id).await {
        Ok(domain) => domain,
        Err(e) => return crate::envelope::error_response(e),
    };
    match domain {
        wf_api::ExecutionDomain::Workflow => {
            match wf_api::workflow::workflow_execution::resume(&state.ctx, &path.id).await {
                Ok(output) => crate::envelope::ok(UnifiedResumeView {
                    execution_id: output.execution_id.to_string(),
                    execution_type: execution_type_of(domain),
                    result: Some(output.result),
                })
                .into_response(),
                Err(e) => crate::envelope::error_response(e),
            }
        }
        wf_api::ExecutionDomain::AgentLoop => {
            match wf_api::agent::agent_execution::resume(&state.ctx, &path.id).await {
                Ok(()) => crate::envelope::ok(UnifiedResumeView {
                    execution_id: path.id,
                    execution_type: execution_type_of(domain),
                    result: None,
                })
                .into_response(),
                Err(e) => crate::envelope::error_response(e),
            }
        }
    }
}

/// Rerun one execution without caring which engine owns it. Only workflow
/// executions support rerun; agent loops are rejected with a validation
/// error pointing at their fork endpoint, mirroring the workflow-only
/// rerun route.
#[utoipa::path(
    post,
    path = "/api/v1/unified-executions/{id}/rerun",
    tag = "observation",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::observation::listing::UnifiedResumeView>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Rerun unsupported for this executor", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unified_rerun(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return crate::envelope::error_response(e);
    }
    match wf_api::resolve_execution(&state.ctx, &path.id).await {
        Ok(wf_api::ExecutionDomain::Workflow) => {}
        Ok(wf_api::ExecutionDomain::AgentLoop) => {
            return crate::envelope::error_response(wf_api::ApiError::Validation(
                wf_api::execution_listing::rerun_unsupported_hint().to_string(),
            ));
        }
        Err(e) => return crate::envelope::error_response(e),
    }
    match wf_api::workflow::workflow_execution::rerun(&state.ctx, &path.id).await {
        Ok(output) => crate::envelope::ok(UnifiedResumeView {
            execution_id: output.execution_id.to_string(),
            execution_type: wf_types::execution::ExecutionType::Workflow,
            result: Some(output.result),
        })
        .into_response(),
        Err(e) => crate::envelope::error_response(e),
    }
}
