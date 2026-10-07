//! Execution log query: uniform log entries projected from lifecycle
//! events, oldest first, with cursor paging. The per-execution route tails
//! one run; the global route reads across runs with an optional execution
//! filter.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use wf_api::execution_logs::{LogEntry, LogFilter};

use crate::envelope::{err, ApiError};
use crate::extract::IdPath;
use crate::paged::{ok_cursor_page, resolve_cursor_page};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/executions/{id}/logs", get(handle_execution_logs))
        .route("/logs", get(handle_query_logs))
}

/// One log line projected from a lifecycle event.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct LogEntryDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workflow_id: Option<String>,
    timestamp: i64,
    event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_name: Option<String>,
    message: String,
}

impl From<LogEntry> for LogEntryDoc {
    fn from(entry: LogEntry) -> Self {
        Self {
            execution_id: entry.execution_id,
            workflow_id: entry.workflow_id,
            timestamp: entry.timestamp,
            event_type: entry.event_type,
            event_name: entry.event_name,
            message: entry.message,
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ExecutionLogsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Opaque cursor from a previous page
    cursor: Option<String>,
    /// Only these event types (comma separated)
    event_types: Option<String>,
    /// Only entries at or after this timestamp (ms epoch)
    since: Option<i64>,
    /// Case-insensitive substring match against the message
    message: Option<String>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct QueryLogsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Opaque cursor from a previous page
    cursor: Option<String>,
    /// Only entries of this execution
    execution_id: Option<String>,
    /// Only these event types (comma separated)
    event_types: Option<String>,
    /// Only entries at or after this timestamp (ms epoch)
    since: Option<i64>,
    /// Case-insensitive substring match against the message
    message: Option<String>,
}

fn parse_event_types(
    raw: Option<&str>,
) -> Result<Option<Vec<wf_types::events::EventType>>, String> {
    match raw {
        None => Ok(None),
        Some(list) => list
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                serde_json::from_value::<wf_types::events::EventType>(serde_json::Value::String(
                    s.to_string(),
                ))
                .map_err(|_| format!("unknown event type: {s}"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/logs",
    tag = "observation",
    params(IdPath, ExecutionLogsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::CursorPageView<crate::api::observation::logs::LogEntryDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execution_logs(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ExecutionLogsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = match resolve_cursor_page(query.limit, query.cursor.as_deref()) {
        Ok(page) => page,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    let event_types = match parse_event_types(query.event_types.as_deref()) {
        Ok(types) => types,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    let filter = LogFilter {
        execution_id: None,
        event_types,
        since: query.since,
        message_contains: query.message,
    };
    let fetch = (limit as usize).saturating_add(1);
    match wf_api::execution_logs::logs_for_execution(
        &state.ctx,
        &path.id,
        Some(&filter),
        fetch,
        offset as usize,
    )
    .await
    {
        Ok((entries, _)) => {
            let window = entries.into_iter().map(LogEntryDoc::from).collect();
            ok_cursor_page(window, limit, offset).into_response()
        }
        Err(e) => crate::envelope::error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/logs",
    tag = "observation",
    params(QueryLogsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::CursorPageView<crate::api::observation::logs::LogEntryDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_query_logs(
    State(state): State<ApiState>,
    Query(query): Query<QueryLogsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = match resolve_cursor_page(query.limit, query.cursor.as_deref()) {
        Ok(page) => page,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    let event_types = match parse_event_types(query.event_types.as_deref()) {
        Ok(types) => types,
        Err(message) => return err(ApiError::validation(message)).into_response(),
    };
    let filter = LogFilter {
        execution_id: query.execution_id,
        event_types,
        since: query.since,
        message_contains: query.message,
    };
    let fetch = (limit as usize).saturating_add(1);
    match wf_api::execution_logs::query_logs(&state.ctx, Some(&filter), fetch, offset as usize)
        .await
    {
        Ok((entries, _)) => {
            let window = entries.into_iter().map(LogEntryDoc::from).collect();
            ok_cursor_page(window, limit, offset).into_response()
        }
        Err(e) => crate::envelope::error_response(e),
    }
}
