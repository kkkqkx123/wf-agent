//! Execution artifact query: named outputs of a run (workflow
//! input/output, per-node results, variables, agent iteration responses
//! and tool results) with truncated previews, oldest source first, with
//! cursor paging. The per-execution route reads one run; the global route
//! scans the executions on the requested listing page.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use wf_api::execution_artifacts::{ArtifactEntry, ArtifactFilter, ArtifactKind};

use crate::envelope::{err, ApiError};
use crate::extract::IdPath;
use crate::paged::{ok_cursor_page, resolve_cursor_page};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/executions/{id}/artifacts",
            get(handle_execution_artifacts),
        )
        .route("/artifacts", get(handle_query_artifacts))
}

/// One named output of an execution.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ArtifactEntryDoc {
    execution_id: String,
    #[schema(value_type = String)]
    execution_type: wf_types::execution::ExecutionType,
    name: String,
    #[schema(value_type = String)]
    kind: ArtifactKind,
    preview: String,
    truncated: bool,
    size_bytes: usize,
}

impl From<ArtifactEntry> for ArtifactEntryDoc {
    fn from(entry: ArtifactEntry) -> Self {
        Self {
            execution_id: entry.execution_id,
            execution_type: entry.execution_type,
            name: entry.name,
            kind: entry.kind,
            preview: entry.preview,
            truncated: entry.truncated,
            size_bytes: entry.size_bytes,
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ExecutionArtifactsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Opaque cursor from a previous page
    cursor: Option<String>,
    /// Only this artifact kind
    kind: Option<String>,
    /// Case-insensitive substring match against the name
    name: Option<String>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct QueryArtifactsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Opaque cursor from a previous page
    cursor: Option<String>,
    /// Only artifacts of this execution
    execution_id: Option<String>,
    /// Only this artifact kind
    kind: Option<String>,
    /// Case-insensitive substring match against the name
    name: Option<String>,
}

fn parse_kind(raw: Option<&str>) -> Result<Option<ArtifactKind>, String> {
    match raw {
        None => Ok(None),
        Some(value) => match value {
            "workflow_input" => Ok(Some(ArtifactKind::WorkflowInput)),
            "workflow_output" => Ok(Some(ArtifactKind::WorkflowOutput)),
            "node_result" => Ok(Some(ArtifactKind::NodeResult)),
            "variable" => Ok(Some(ArtifactKind::Variable)),
            "iteration_response" => Ok(Some(ArtifactKind::IterationResponse)),
            "tool_result" => Ok(Some(ArtifactKind::ToolResult)),
            other => Err(format!("unknown artifact kind: {other}")),
        },
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/artifacts",
    tag = "observation",
    params(IdPath, ExecutionArtifactsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::CursorPageView<crate::api::observation::artifacts::ArtifactEntryDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execution_artifacts(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ExecutionArtifactsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = match resolve_cursor_page(query.limit, query.cursor.as_deref()) {
        Ok(page) => page,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    let kind = match parse_kind(query.kind.as_deref()) {
        Ok(kind) => kind,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    let filter = ArtifactFilter {
        execution_id: Some(path.id.clone()),
        kind,
        name_contains: query.name,
        preview_contains: None,
    };
    let fetch = (limit as usize).saturating_add(1);
    match wf_api::execution_artifacts::query_artifacts(
        &state.ctx,
        Some(&filter),
        fetch,
        offset as usize,
    )
    .await
    {
        Ok((entries, _)) => {
            let window = entries.into_iter().map(ArtifactEntryDoc::from).collect();
            ok_cursor_page(window, limit, offset).into_response()
        }
        Err(e) => crate::envelope::error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/artifacts",
    tag = "observation",
    params(QueryArtifactsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::CursorPageView<crate::api::observation::artifacts::ArtifactEntryDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_query_artifacts(
    State(state): State<ApiState>,
    Query(query): Query<QueryArtifactsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = match resolve_cursor_page(query.limit, query.cursor.as_deref()) {
        Ok(page) => page,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    let kind = match parse_kind(query.kind.as_deref()) {
        Ok(kind) => kind,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    let filter = ArtifactFilter {
        execution_id: query.execution_id,
        kind,
        name_contains: query.name,
        preview_contains: None,
    };
    let fetch = (limit as usize).saturating_add(1);
    match wf_api::execution_artifacts::query_artifacts(
        &state.ctx,
        Some(&filter),
        fetch,
        offset as usize,
    )
    .await
    {
        Ok((entries, _)) => {
            let window = entries.into_iter().map(ArtifactEntryDoc::from).collect();
            ok_cursor_page(window, limit, offset).into_response()
        }
        Err(e) => crate::envelope::error_response(e),
    }
}
