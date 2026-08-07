//! HTTP transport for the application-facing `wf-api` surface: execution,
//! query and event-stream endpoints (the Stage 6 acceptance surface).
//!
//! `api_router` composes a set of REST + SSE handlers over an
//! `Arc<wf_api::ApiContext>`; `serve_api` binds them to a TCP listener with
//! graceful shutdown. Metrics endpoints (wf-server/http.rs) can be merged
//! through [`full_router`] / [`serve_full`].

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use wf_api::ApiContext;
use wf_api::EventSubscriptionOptions;

use crate::http::{ok, ServeError, ServerHandle};

#[derive(Clone)]
struct ApiState {
    ctx: Arc<ApiContext>,
}

/// Build the `wf-api` router (execution / query / event stream).
pub fn api_router(ctx: Arc<ApiContext>) -> Router {
    Router::new()
        .route("/health", get(handle_health))
        .route("/workflows", get(handle_list_workflows))
        .route("/workflows/{id}", get(handle_get_workflow))
        .route("/workflows/{id}/execute", post(handle_execute_workflow))
        .route("/executions", get(handle_list_executions))
        .route("/executions/{id}", get(handle_get_execution))
        .route("/events", get(handle_list_events))
        .route("/events/stats", get(handle_event_stats))
        .route("/events/stream", get(handle_event_stream))
        .route("/agent-executions", get(handle_agent_executions))
        .with_state(ApiState { ctx })
}

/// Merge the metrics router and the API router under one listener.
pub fn full_router(registry: Arc<wf_metrics::MetricsRegistry>, ctx: Arc<ApiContext>) -> Router {
    crate::http::router(registry).merge(api_router(ctx))
}

/// Serve the `wf-api` surface on `addr` without blocking.
pub async fn serve_api(ctx: Arc<ApiContext>, addr: SocketAddr) -> Result<ServerHandle, ServeError> {
    serve_with_router(api_router(ctx), addr).await
}

/// Serve metrics + API on the same listener.
pub async fn serve_full(
    registry: Arc<wf_metrics::MetricsRegistry>,
    ctx: Arc<ApiContext>,
    addr: SocketAddr,
) -> Result<ServerHandle, ServeError> {
    serve_with_router(full_router(registry, ctx), addr).await
}

async fn serve_with_router(router: Router, addr: SocketAddr) -> Result<ServerHandle, ServeError> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| ServeError::Bind(e.to_string()))?;
    let bound_addr = listener
        .local_addr()
        .map_err(|e| ServeError::Bind(e.to_string()))?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
        {
            tracing::error!(target: "wf_server", error = %err, "wf-api HTTP server failed");
        }
    });
    Ok(ServerHandle {
        addr: bound_addr,
        shutdown: shutdown_tx,
        task,
    })
}

// ============================================================================
// Handlers
// ============================================================================

#[derive(Serialize)]
pub struct HealthView {
    ready: bool,
    persistence: wf_api::PersistenceHealth,
    storage: String,
}

async fn handle_health(State(state): State<ApiState>) -> impl IntoResponse {
    let snapshot = state.ctx.storage.ops_snapshot();
    let total_ops = snapshot.save.count()
        + snapshot.load.count()
        + snapshot.delete.count()
        + snapshot.list.count()
        + snapshot.exists.count()
        + snapshot.clear.count()
        + snapshot.batch.count();
    ok(HealthView {
        ready: true,
        persistence: state.ctx.persistence.health(),
        storage: format!("{total_ops} ops"),
    })
    .into_response()
}

async fn handle_list_workflows(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::workflow::workflow::list_workflows(&state.ctx, None).await {
        Ok(workflows) => ok(workflows).into_response(),
        Err(e) => crate::http::error_response(e),
    }
}

#[derive(Deserialize)]
struct WorkflowPath {
    id: String,
}

async fn handle_get_workflow(
    State(state): State<ApiState>,
    Path(path): Path<WorkflowPath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow::get_workflow(&state.ctx, &path.id).await {
        Ok(workflow) => ok(workflow).into_response(),
        Err(e) => crate::http::error_response(e),
    }
}

#[derive(Deserialize)]
pub struct ExecuteBody {
    input: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct ExecuteView {
    execution_id: String,
    result: serde_json::Value,
}

async fn handle_execute_workflow(
    State(state): State<ApiState>,
    Path(path): Path<WorkflowPath>,
    Json(body): Json<ExecuteBody>,
) -> impl IntoResponse {
    let params = wf_api::workflow::workflow_execution::ExecuteWorkflowParams {
        workflow_id: path.id,
        input: body.input,
        options: None,
    };
    match wf_api::workflow::workflow_execution::execute(&state.ctx, params).await {
        Ok(output) => ok(ExecuteView {
            execution_id: output.execution_id.to_string(),
            result: output.result,
        })
        .into_response(),
        Err(e) => crate::http::error_response(e),
    }
}

#[derive(Deserialize)]
struct ListExecutionsQuery {
    workflow_id: Option<String>,
    limit: Option<u64>,
}

async fn handle_list_executions(
    State(state): State<ApiState>,
    Query(query): Query<ListExecutionsQuery>,
) -> impl IntoResponse {
    let options = wf_api::WorkflowExecutionListOptions {
        workflow_id_filter: query.workflow_id,
        limit: query.limit,
        ..Default::default()
    };
    match wf_api::workflow::workflow::list_executions(&state.ctx, Some(options)).await {
        Ok(executions) => ok(executions).into_response(),
        Err(e) => crate::http::error_response(e),
    }
}

async fn handle_get_execution(
    State(state): State<ApiState>,
    Path(path): Path<WorkflowPath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow::get_execution(&state.ctx, &path.id).await {
        Ok(execution) => ok(execution).into_response(),
        Err(e) => crate::http::error_response(e),
    }
}

#[derive(Deserialize)]
struct ListEventsQuery {
    execution_id: Option<String>,
    agent_loop_id: Option<String>,
    workflow_id: Option<String>,
    limit: Option<usize>,
}

async fn handle_list_events(
    State(state): State<ApiState>,
    Query(query): Query<ListEventsQuery>,
) -> impl IntoResponse {
    let options = wf_api::EventQueryOptions {
        execution_id: query.execution_id,
        agent_loop_id: query.agent_loop_id,
        workflow_id: query.workflow_id,
        limit: query.limit,
        event_types: None,
    };
    match wf_api::infra::events::history(&state.ctx, &options).await {
        Ok(events) => ok(events).into_response(),
        Err(e) => crate::http::error_response(e),
    }
}

async fn handle_event_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::infra::events::get_event_stats(&state.ctx, &wf_api::EventQueryOptions::default())
        .await
    {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => crate::http::error_response(e),
    }
}

#[derive(Deserialize)]
struct StreamEventsQuery {
    execution_id: Option<String>,
    agent_loop_id: Option<String>,
    workflow_id: Option<String>,
}

async fn handle_event_stream(
    State(state): State<ApiState>,
    Query(query): Query<StreamEventsQuery>,
) -> Response {
    let options = EventSubscriptionOptions {
        execution_id: query.execution_id,
        agent_loop_id: query.agent_loop_id,
        workflow_id: query.workflow_id,
        event_types: None,
    };
    let sub = wf_api::infra::events::subscribe(&state.ctx, options);
    let stream = futures::stream::unfold(sub, |mut sub| async move {
        match sub.next().await {
            Some(event) => {
                let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
                let frame = format!("data: {payload}\n\n");
                Some((Ok::<_, Infallible>(axum::body::Bytes::from(frame)), sub))
            }
            None => None,
        }
    });

    let response = Response::new(Body::from_stream(stream));
    let (mut parts, body) = response.into_parts();
    parts.status = StatusCode::OK;
    parts.headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/event-stream"),
    );
    parts.headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-cache"),
    );
    Response::from_parts(parts, body)
}

async fn handle_agent_executions(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::agent::agent_execution_registry::summaries(&state.ctx, None).await {
        Ok(summaries) => ok(summaries).into_response(),
        Err(e) => crate::http::error_response(e),
    }
}

// ============================================================================
// Tests
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body as AxBody;
    use axum::http::Request;
    use futures::StreamExt;
    use tower::ServiceExt;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registrar::Registries::new()),
            Arc::new(wf_resource::starter::BundleRegistry::new()),
        ))
    }

    async fn get(ctx: Arc<ApiContext>, uri: &str) -> Response {
        api_router(ctx)
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
    async fn health_endpoint_reports_ready() {
        let ctx = make_ctx();
        let response = get(ctx, "/health").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["data"]["ready"], true);
    }

    #[tokio::test]
    async fn workflows_and_executions_are_queryable() {
        let ctx = make_ctx();

        let workflows = get(ctx.clone(), "/workflows").await;
        assert_eq!(workflows.status(), StatusCode::OK);

        let executions = get(ctx.clone(), "/executions").await;
        assert_eq!(executions.status(), StatusCode::OK);

        let agent_executions = get(ctx.clone(), "/agent-executions").await;
        assert_eq!(agent_executions.status(), StatusCode::OK);

        // Missing execution id maps onto the NotFound envelope.
        let missing = get(ctx, "/executions/nope").await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let body = json_body(missing).await;
        assert_eq!(body["success"], false);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
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
                execution_id: None,
                agent_loop_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();

        let events = get(ctx.clone(), "/events").await;
        assert_eq!(events.status(), StatusCode::OK);
        let body = json_body(events).await;
        assert_eq!(body["data"].as_array().unwrap().len(), 1);

        let stats = get(ctx, "/events/stats").await;
        assert_eq!(stats.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn event_stream_emits_sse_frames() {
        let ctx = make_ctx();

        let client = tokio::spawn({
            let ctx = ctx.clone();
            async move {
                let response = get(ctx, "/events/stream").await;
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(
                    response.headers().get("content-type").unwrap(),
                    "text/event-stream"
                );
                let mut stream = response.into_body().into_data_stream();
                let first_frame = stream.next().await.unwrap().unwrap();
                String::from_utf8(first_frame.to_vec()).unwrap()
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
                metadata: None,
            },
        )
        .await
        .unwrap();

        let body = client.await.unwrap();
        assert!(body.starts_with("data: "), "SSE frame: {body}");
        assert!(body.contains("NODE_STARTED"), "SSE body: {body}");
    }
}
