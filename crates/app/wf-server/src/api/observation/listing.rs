//! Unified execution listing: one time-ordered view over workflow runs
//! and agent loop runs. Per-engine lists stay where they are; this route
//! merges both sides for callers that do not care which engine owns a run.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use wf_api::execution_listing::{UnifiedExecutionFilter, UnifiedExecutionSummary};

use crate::envelope::{err, ApiError};
use crate::paged::{ok_cursor_page, resolve_cursor_page};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/unified-executions", get(handle_unified_executions))
        .route("/unified-executions/count", get(handle_unified_executions_count))
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
    /// Filter by engine (`workflow` or `agent_loop`)
    execution_type: Option<String>,
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
    /// Filter by engine (`workflow` or `agent_loop`)
    execution_type: Option<String>,
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
            parent_execution_id: summary.parent_execution_id,
            error: summary.error,
        }
    }
}

fn parse_filter(
    status: Option<&str>,
    execution_type: Option<&str>,
    started_from: Option<i64>,
    started_to: Option<i64>,
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
        Some(raw) if raw.eq_ignore_ascii_case("workflow") => {
            Some(wf_types::execution::ExecutionType::Workflow)
        }
        Some(raw) if raw.eq_ignore_ascii_case("agent_loop") => {
            Some(wf_types::execution::ExecutionType::AgentLoop)
        }
        Some(raw) => return Err(format!("unknown execution_type: {raw}")),
    };
    Ok(UnifiedExecutionFilter {
        status,
        execution_type,
        started_from,
        started_to,
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
    Query(query): Query<UnifiedExecutionsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = match resolve_cursor_page(query.limit, query.cursor.as_deref()) {
        Ok(page) => page,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    let filter = match parse_filter(
        query.status.as_deref(),
        query.execution_type.as_deref(),
        query.started_from,
        query.started_to,
    ) {
        Ok(filter) => filter,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    // Ask for one extra item past the page: `from_window` turns the extra
    // item into `has_more` plus the following cursor and drops it.
    let fetch = (limit as usize).saturating_add(1);
    match wf_api::execution_listing::list_unified(
        &state.ctx,
        Some(&filter),
        fetch,
        offset as usize,
    )
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
    Query(query): Query<UnifiedExecutionsCountQuery>,
) -> impl IntoResponse {
    let filter = match parse_filter(
        query.status.as_deref(),
        query.execution_type.as_deref(),
        query.started_from,
        query.started_to,
    ) {
        Ok(filter) => filter,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    match wf_api::execution_listing::count_unified(&state.ctx, Some(&filter)).await {
        Ok(count) => crate::envelope::ok(UnifiedExecutionCountView { count }).into_response(),
        Err(e) => crate::envelope::error_response(e),
    }
}
