//! Execution domain: workflow execution trigger + control (pause/resume/
//! cancel/status/stream), execution list/detail and execution-scoped trigger
//! history. Checkpoint / state-view / graph-analysis surfaces live in the
//! sibling modules `api_checkpoints`, `api_execution_state` and
//! `api_execution_analysis`.

use std::convert::Infallible;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use wf_storage::adapter::execution::WorkflowExecutionListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{ExecutionIdPath, IdPath, ListQuery};
use crate::router::ApiState;
use crate::sse::sse_response;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .merge(crate::api::workflow::checkpoints::routes())
        .merge(crate::api::workflow::execution_state::routes())
        .merge(crate::api::workflow::execution_analysis::routes())
        // ── execution trigger + stream ──
        .route("/workflows/{id}/execute", post(handle_execute_workflow))
        .route(
            "/workflows/{id}/execute/stream",
            post(handle_execute_stream),
        )
        // ── execution list / detail / control ──
        .route("/executions", get(handle_list_executions))
        .route(
            "/executions/{id}",
            get(handle_get_execution).delete(handle_delete_execution),
        )
        .route("/executions/{id}/pause", post(handle_pause))
        .route("/executions/{id}/resume", post(handle_resume))
        .route("/executions/{id}/cancel", post(handle_cancel))
        .route("/executions/{id}/status", get(handle_status))
        // ── execution triggers ──
        .route("/executions/{id}/triggers", get(handle_trigger_history))
        .route(
            "/execution-triggers/{id}/enable",
            post(handle_trigger_enable),
        )
        .route(
            "/execution-triggers/{id}/disable",
            post(handle_trigger_disable),
        )
}

#[derive(Deserialize)]
pub struct ExecuteBody {
    pub(crate) input: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct ExecuteView {
    pub(crate) execution_id: String,
    pub(crate) result: serde_json::Value,
}

#[derive(Serialize)]
pub struct StreamMetadataView {
    execution_id: String,
}

async fn handle_execute_workflow(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
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
        Err(e) => error_response(e),
    }
}

async fn handle_execute_stream(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<ExecuteBody>,
) -> Response {
    let params = wf_api::workflow::workflow_execution::ExecuteWorkflowParams {
        workflow_id: path.id,
        input: body.input,
        options: None,
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
    let events = futures::stream::unfold(stream, |mut stream| async move {
        match stream.next().await {
            Some(event) => {
                let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
                let frame = format!("data: {payload}\n\n");
                Some((Ok::<_, Infallible>(Bytes::from(frame)), stream))
            }
            None => None,
        }
    });
    sse_response(futures::stream::once(futures::future::ready(first)).chain(events))
}

#[derive(Deserialize)]
struct ListExecutionsQuery {
    #[serde(flatten)]
    page: ListQuery,
    workflow_id: Option<String>,
    status: Option<String>,
}

async fn handle_list_executions(
    State(state): State<ApiState>,
    Query(query): Query<ListExecutionsQuery>,
) -> impl IntoResponse {
    let options = WorkflowExecutionListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        workflow_id_filter: query.workflow_id,
        status_filter: query.status,
    };
    match wf_api::workflow::list_executions(&state.ctx, Some(options)).await {
        Ok(executions) => ok(executions).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_execution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::get_execution(&state.ctx, &path.id).await {
        Ok(execution) => ok(execution).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_execution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::delete_execution(&state.ctx, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_pause(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_execution::pause(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_resume(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_execution::resume(&state.ctx, &path.id).await {
        Ok(output) => ok(ExecuteView {
            execution_id: output.execution_id.to_string(),
            result: output.result,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_cancel(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_execution::cancel(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_status(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_execution::status(&state.ctx, &path.id).await {
        Ok(status) => ok(status).into_response(),
        Err(e) => error_response(e),
    }
}

// ── execution triggers ────────────────────────────────────────────

#[derive(Deserialize)]
struct TriggerHistoryQuery {
    trigger_name: Option<String>,
}

async fn handle_trigger_history(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<TriggerHistoryQuery>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_trigger::trigger_execution_history(
        &state.ctx,
        &path.id,
        query.trigger_name.as_deref(),
    )
    .await
    {
        Ok(history) => ok(history).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_enable(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_trigger::enable(&state.ctx, &path.execution_id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_disable(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_trigger::disable(&state.ctx, &path.execution_id).await {
        Ok(()) => ok(()).into_response(),
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
        // Read / analysis views degrade to empty results for unknown
        // executions instead of failing.
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
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
        // Graph reconstruction degrades to an empty graph for unknown
        // executions instead of failing.
        let graph = send(ctx.clone(), "GET", "/api/v1/executions/missing/graph").await;
        assert_eq!(graph.status(), StatusCode::OK);

        // Analysis / state views over an unknown execution degrade to empty
        // results rather than NotFound.
        for uri in [
            "/api/v1/executions/missing/context-transitions",
            "/api/v1/executions/missing/context-snapshots",
            "/api/v1/executions/missing/agent-state",
            "/api/v1/executions/missing/agent-iterations",
            "/api/v1/executions/missing/agent-variables",
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
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
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
        assert_eq!(chain.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn slow_nodes_rejects_out_of_range_percentile() {
        let ctx = make_ctx();
        let response = send(
            ctx,
            "GET",
            "/api/v1/executions/x/analysis/slow-nodes?percentile=150",
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn state_records_endpoints_are_queryable() {
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
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
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
