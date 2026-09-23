//! Events domain: event history, statistics, the SSE event stream and the
//! timeline / search / size / time-range extensions.

use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use utoipa::IntoParams;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use wf_api::EventSubscriptionOptions;

use crate::envelope::{err, error_response, ok, ApiError};
use crate::extract::{ExecutionIdPath, IdPath, ListQuery};
use crate::paged::{
    fetch_size, ok_capped, ok_page, resolve_page, resolve_page_fields, MAX_TIMELINE_ENTRIES,
};
use crate::router::ApiState;
use crate::sse::sse_response;

/// Max concurrent SSE connections, default of 100.
const MAX_SSE_CLIENTS: usize = 100;
/// Max events pulled for one `GET /events` page (offset + limit + 1).
const MAX_EVENT_FETCH: usize = 5000;
static SSE_CLIENTS: AtomicUsize = AtomicUsize::new(0);

/// Decrements the SSE client counter when the response body is dropped
/// (client disconnect or stream end).
pub(crate) struct SseClientGuard;

impl Drop for SseClientGuard {
    fn drop(&mut self) {
        SSE_CLIENTS.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/events",
            get(handle_list_events).delete(handle_clear_events),
        )
        .route("/events/stats", get(handle_event_stats))
        .route("/events/stream", get(handle_event_stream))
        .route("/events/search", get(handle_search_events))
        .route("/events/size", get(handle_event_size))
        .route("/events/time-range", get(handle_event_time_range))
        .route(
            "/events/timeline/{executionId}",
            get(handle_execution_timeline),
        )
        .route("/events/agent-timeline/{id}", get(handle_agent_timeline))
        .route(
            "/events/execution-timeline/{executionId}",
            get(handle_execution_timeline_view),
        )
        .route(
            "/events/execution-timeline/{executionId}/summary",
            get(handle_execution_timeline_summary),
        )
        .route(
            "/events/listener-stats/{executionId}",
            get(handle_listener_stats),
        )
        .route("/events/agent/stats", get(handle_agent_loop_statistics))
        .route("/events/agent/{agentLoopId}", get(handle_agent_events))
        .route(
            "/events/agent/{agentLoopId}/turns",
            get(handle_agent_turn_events),
        )
        .route(
            "/events/agent/{agentLoopId}/tool-executions",
            get(handle_agent_tool_execution_events),
        )
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListEventsQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    execution_id: Option<String>,
    agent_loop_id: Option<String>,
    workflow_id: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/events",
    tag = "system",
    params(ListEventsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_events(
    State(state): State<ApiState>,
    Query(query): Query<ListEventsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    // `history` truncates to `limit` from the start, so deep pages must
    // over-fetch past the offset; the fetch stays bounded.
    let fetch = offset
        .saturating_add(limit)
        .saturating_add(1)
        .min(MAX_EVENT_FETCH as u64) as usize;
    let options = wf_api::EventQueryOptions {
        execution_id: query.execution_id,
        agent_loop_id: query.agent_loop_id,
        workflow_id: query.workflow_id,
        limit: Some(fetch),
        event_types: None,
    };
    match wf_api::infra::events::history(&state.ctx, &options).await {
        Ok(events) => {
            let window = events
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ClearEventsQuery {
    force: Option<bool>,
}

#[utoipa::path(
    delete,
    path = "/api/v1/events",
    tag = "system",
    params(ClearEventsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_clear_events(
    State(state): State<ApiState>,
    Query(query): Query<ClearEventsQuery>,
) -> impl IntoResponse {
    if !query.force.unwrap_or(false) {
        return err(ApiError::validation(
            "clearing the event history requires ?force=true",
        ))
        .into_response();
    }
    match wf_api::infra::events::clear_event_history(&state.ctx).await {
        Ok(count) => ok(count).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/events/stats",
    tag = "system",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_event_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::infra::events::get_event_stats(&state.ctx, &wf_api::EventQueryOptions::default())
        .await
    {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct SearchEventsQuery {
    q: String,
    execution_id: Option<String>,
    agent_loop_id: Option<String>,
    workflow_id: Option<String>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/events/search",
    tag = "system",
    params(SearchEventsQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_search_events(
    State(state): State<ApiState>,
    Query(query): Query<SearchEventsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    let fetch = offset
        .saturating_add(limit)
        .saturating_add(1)
        .min(MAX_EVENT_FETCH as u64) as usize;
    let options = wf_api::EventQueryOptions {
        execution_id: query.execution_id,
        agent_loop_id: query.agent_loop_id,
        workflow_id: query.workflow_id,
        limit: Some(fetch),
        event_types: None,
    };
    match wf_api::infra::events::search_events(&state.ctx, &query.q, &options).await {
        Ok(events) => {
            let window = events
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
    path = "/api/v1/events/size",
    tag = "system",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_event_size(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::infra::events::event_history_size(&state.ctx).await {
        Ok(size) => ok(size).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/events/time-range",
    tag = "system",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_event_time_range(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::infra::events::event_time_range(&state.ctx).await {
        Ok(range) => ok(range).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/events/timeline/{executionId}",
    tag = "system",
    params(ExecutionIdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::CappedView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execution_timeline(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::infra::events::timeline(&state.ctx, &path.execution_id).await {
        Ok(events) => ok_capped(events, MAX_TIMELINE_ENTRIES).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/events/agent-timeline/{id}",
    tag = "system",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::CappedView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_agent_timeline(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::infra::events::agent_timeline(&state.ctx, &path.id).await {
        Ok(events) => ok_capped(events, MAX_TIMELINE_ENTRIES).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/events/execution-timeline/{executionId}",
    tag = "system",
    params(ExecutionIdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execution_timeline_view(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::infra::events::get_execution_timeline(&state.ctx, &path.execution_id).await {
        Ok(Some(timeline)) => ok(cap_execution_timeline(timeline)).into_response(),
        Ok(None) => ok(serde_json::Value::Null).into_response(),
        Err(e) => error_response(e),
    }
}

/// Capped execution timeline view with an explicit truncation flag and
/// pre-truncation total.
#[derive(Serialize)]
pub(crate) struct CappedExecutionTimelineView {
    execution_id: String,
    workflow_id: Option<String>,
    status: String,
    start_time: i64,
    end_time: i64,
    total_elapsed: i64,
    phases: Vec<wf_api::infra::events::ExecutionTimelinePhase>,
    events: Vec<wf_types::events::BaseEvent>,
    truncated: bool,
    total: usize,
}

fn cap_execution_timeline(
    timeline: wf_api::infra::events::ExecutionTimeline,
) -> CappedExecutionTimelineView {
    let total = timeline.events.len();
    let truncated = total > MAX_TIMELINE_ENTRIES;
    let mut events = timeline.events;
    events.truncate(MAX_TIMELINE_ENTRIES);
    CappedExecutionTimelineView {
        execution_id: timeline.execution_id,
        workflow_id: timeline.workflow_id,
        status: timeline.status,
        start_time: timeline.start_time,
        end_time: timeline.end_time,
        total_elapsed: timeline.total_elapsed,
        phases: timeline.phases,
        events,
        truncated,
        total,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/events/execution-timeline/{executionId}/summary",
    tag = "system",
    params(ExecutionIdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execution_timeline_summary(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    // Compact counts-only digest with no event dump; retained as a single
    // object with no pagination or cap.
    match wf_api::infra::events::execution_timeline_summary(&state.ctx, &path.execution_id).await {
        Ok(summary) => ok(summary).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/events/listener-stats/{executionId}",
    tag = "system",
    params(ExecutionIdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_listener_stats(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::infra::events::execution_listener_stats(&state.ctx, &path.execution_id).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/events/agent/stats",
    tag = "system",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_agent_loop_statistics(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    match wf_api::infra::events::get_agent_loop_statistics(&state.ctx).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Path)]
pub(crate) struct AgentLoopPath {
    agent_loop_id: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/events/agent/{agentLoopId}",
    tag = "system",
    params(AgentLoopPath, ListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_agent_events(
    State(state): State<ApiState>,
    Path(path): Path<AgentLoopPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::infra::events::get_agent_events(&state.ctx, &path.agent_loop_id).await {
        Ok(events) => {
            let (limit, offset) = resolve_page(&query);
            let window = events
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
    path = "/api/v1/events/agent/{agentLoopId}/turns",
    tag = "system",
    params(AgentLoopPath, ListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_agent_turn_events(
    State(state): State<ApiState>,
    Path(path): Path<AgentLoopPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::infra::events::get_agent_turn_events(&state.ctx, &path.agent_loop_id).await {
        Ok(events) => {
            let (limit, offset) = resolve_page(&query);
            let window = events
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
    path = "/api/v1/events/agent/{agentLoopId}/tool-executions",
    tag = "system",
    params(AgentLoopPath, ListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_agent_tool_execution_events(
    State(state): State<ApiState>,
    Path(path): Path<AgentLoopPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::infra::events::get_agent_tool_execution_events(&state.ctx, &path.agent_loop_id)
        .await
    {
        Ok(events) => {
            let (limit, offset) = resolve_page(&query);
            let window = events
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct StreamEventsQuery {
    execution_id: Option<String>,
    agent_loop_id: Option<String>,
    workflow_id: Option<String>,
    /// Opaque reconnect cursor (same hex token as the websocket `cursor`);
    /// raw timestamps stay accepted for compat but are not documented.
    since: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/events/stream",
    tag = "system",
    params(StreamEventsQuery),
    responses(
        (status = 200, description = "Server-sent events stream", content_type = "text/event-stream"),
        (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_event_stream(
    State(state): State<ApiState>,
    Query(query): Query<StreamEventsQuery>,
) -> Response {
    if SSE_CLIENTS.fetch_add(1, Ordering::SeqCst) >= MAX_SSE_CLIENTS {
        SSE_CLIENTS.fetch_sub(1, Ordering::SeqCst);
        tracing::warn!(target: "wf_server", "SSE connection rejected: max connections ({MAX_SSE_CLIENTS}) reached");
        return crate::envelope::service_unavailable("Too many SSE connections");
    }
    // Client disconnect drops the guard and the stream, cancelling the
    // subscription immediately; no explicit cancel frame is needed.
    let _guard = SseClientGuard;

    let execution_id = query.execution_id.clone();
    let options = EventSubscriptionOptions {
        execution_id: query.execution_id.clone(),
        agent_loop_id: query.agent_loop_id.clone(),
        workflow_id: query.workflow_id.clone(),
        event_types: None,
    };
    let replay_query = wf_api::EventQueryOptions {
        execution_id: query.execution_id,
        agent_loop_id: query.agent_loop_id,
        workflow_id: query.workflow_id,
        event_types: None,
        limit: Some(500),
    };
    let since = query.since.as_deref().and_then(crate::ws::decode_sse_since);
    let ctx = state.ctx.clone();
    // Bounded replay of the retained window for reconnects.
    let backlog: Vec<wf_types::events::BaseEvent> =
        match wf_api::infra::events::history(&ctx, &replay_query).await {
            Ok(mut events) => {
                events.sort_by_key(|e| e.timestamp);
                events
                    .into_iter()
                    .filter(|e| since.map(|after| e.timestamp > after).unwrap_or(false))
                    .take(200)
                    .collect()
            }
            Err(_) => Vec::new(),
        };
    let sub = wf_api::infra::events::subscribe(&state.ctx, options);
    // Initial connection event plus the bounded reconnect backlog.
    let connected = futures::stream::once(async move {
        let payload = serde_json::json!({
            "type": "connected",
            "data": { "executionId": execution_id }
        });
        let frame = format!("data: {payload}\n\n");
        Ok::<_, Infallible>(axum::body::Bytes::from(frame))
    });
    let backlog_stream = futures::stream::iter(backlog.into_iter().map(|event| {
        let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
        let frame = format!("data: {payload}\n\n");
        Ok::<_, Infallible>(axum::body::Bytes::from(frame))
    }));
    let events = futures::stream::unfold(sub, |mut sub| async move {
        match sub.next().await {
            Some(event) => {
                let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
                let frame = format!("data: {payload}\n\n");
                Some((Ok::<_, Infallible>(axum::body::Bytes::from(frame)), sub))
            }
            None => None,
        }
    });
    // Comment-frame keepalive every 30s.
    let keepalive = futures::stream::unfold(
        tokio::time::interval(Duration::from_secs(30)),
        |mut interval| async move {
            interval.tick().await;
            Some((
                Ok::<_, Infallible>(axum::body::Bytes::from_static(b":keepalive\n\n")),
                interval,
            ))
        },
    );
    sse_response(futures::stream::select(
        connected.chain(backlog_stream).chain(events),
        keepalive,
    ))
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::Request;
    use axum::response::Response;
    use futures::StreamExt;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    use super::MAX_SSE_CLIENTS;

    /// Serializes the SSE connection-limit tests: they share the process-wide
    /// `SSE_CLIENTS` counter with the live-stream test, so they must not run
    /// concurrently.
    static SSE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    async fn get(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
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
    async fn events_endpoints_work() {
        let ctx = make_ctx();
        wf_api::infra::events::dispatch(
            &ctx,
            wf_types::events::BaseEvent {
                id: wf_common::generate_id(),
                r#type: wf_types::events::EventType::Heartbeat,
                timestamp: 1,
                workflow_id: None,
                execution_id: Some("exec-events".into()),
                agent_loop_id: None,

                event_name: None,
                metadata: None,
            },
        )
        .await
        .unwrap();

        let events = get(ctx.clone(), "/api/v1/events").await;
        assert_eq!(events.status(), axum::http::StatusCode::OK);
        let body = json_body(events).await;
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["has_more"], false);

        let stats = get(ctx.clone(), "/api/v1/events/stats").await;
        assert_eq!(stats.status(), axum::http::StatusCode::OK);

        for uri in [
            "/api/v1/events/size",
            "/api/v1/events/time-range",
            "/api/v1/events/search?q=heartbeat",
            "/api/v1/events/timeline/exec-events",
            "/api/v1/events/agent-timeline/loop-1",
            "/api/v1/events/execution-timeline/exec-events",
            "/api/v1/events/execution-timeline/exec-events/summary",
            "/api/v1/events/listener-stats/exec-events",
            "/api/v1/events/agent/stats",
            "/api/v1/events/agent/loop-1",
            "/api/v1/events/agent/loop-1/turns",
            "/api/v1/events/agent/loop-1/tool-executions",
        ] {
            let response = get(ctx.clone(), uri).await;
            assert_eq!(response.status(), axum::http::StatusCode::OK, "uri: {uri}");
        }
    }

    #[tokio::test]
    async fn clear_events_requires_force() {
        let ctx = make_ctx();
        let without_force = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/events")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(without_force.status(), axum::http::StatusCode::BAD_REQUEST);

        let with_force = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/events?force=true")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(with_force.status(), axum::http::StatusCode::OK);
        let body = json_body(with_force).await;
        assert_eq!(body["success"], true);
    }

    #[tokio::test]
    async fn event_stream_emits_sse_frames() {
        let _lock = SSE_TEST_LOCK.lock().await;
        let ctx = make_ctx();

        let client = tokio::spawn({
            let ctx = ctx.clone();
            async move {
                let response = get(ctx, "/api/v1/events/stream").await;
                assert_eq!(response.status(), axum::http::StatusCode::OK);
                assert_eq!(
                    response.headers().get("content-type").unwrap(),
                    "text/event-stream"
                );
                let mut stream = response.into_body().into_data_stream();
                let mut frames: Vec<String> = Vec::new();
                for _ in 0..8 {
                    match tokio::time::timeout(std::time::Duration::from_millis(500), stream.next())
                        .await
                    {
                        Ok(Some(Ok(bytes))) => {
                            frames.push(String::from_utf8(bytes.to_vec()).unwrap())
                        }
                        _ => break,
                    }
                }
                frames
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        wf_api::infra::events::dispatch(
            &ctx,
            wf_types::events::BaseEvent {
                id: wf_common::generate_id(),
                r#type: wf_types::events::EventType::NodeStarted,
                timestamp: 1,
                workflow_id: None,
                execution_id: Some("exec-stream".into()),
                agent_loop_id: None,

                event_name: None,
                metadata: None,
            },
        )
        .await
        .unwrap();

        let frames = client.await.unwrap();
        assert!(
            frames.iter().any(|f| f.contains("\"type\":\"connected\"")),
            "expected connected frame, got: {frames:?}"
        );
        assert!(
            frames.iter().any(|f| f.contains("NODE_STARTED")),
            "expected event frame, got: {frames:?}"
        );
    }

    #[tokio::test]
    async fn event_stream_rejects_when_limit_reached() {
        let _lock = SSE_TEST_LOCK.lock().await;
        let ctx = make_ctx();
        // Saturate the client counter (with margin so concurrent tests
        // releasing their connections cannot drop it below the limit).
        super::SSE_CLIENTS.store(MAX_SSE_CLIENTS + 16, Ordering::SeqCst);
        let rejected = get(ctx, "/api/v1/events/stream").await;
        assert_eq!(
            rejected.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
        super::SSE_CLIENTS.store(0, Ordering::SeqCst);
    }

    #[tokio::test]
    async fn sse_guard_decrements_on_drop() {
        let _lock = SSE_TEST_LOCK.lock().await;
        let before = super::SSE_CLIENTS.load(Ordering::SeqCst);
        super::SSE_CLIENTS.fetch_add(1, Ordering::SeqCst);
        {
            let _guard = super::SseClientGuard;
            assert_eq!(super::SSE_CLIENTS.load(Ordering::SeqCst), before + 1);
        }
        assert_eq!(super::SSE_CLIENTS.load(Ordering::SeqCst), before);
    }
}
