//! Router-level contract tests for the execution analysis and decision-graph
//! surface consumed by the web app's analytics screens. With a memory context
//! and no recorded metrics, the stats endpoints report an unconfigured state,
//! LLM metrics return an empty envelope, and an unknown agent loop yields an
//! empty decision graph. The execution-graph not-found branch is already
//! covered in the shared API contract file, so it is not repeated here.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;
use wf_api::ApiContext;
use wf_resource::registry::ResourceRegistries;
use wf_server::middleware::ServerMiddlewareConfig;
use wf_storage::context::StorageContext;

fn app() -> Router {
    let ctx = ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    );
    wf_server::api_router_with_config(Arc::new(ctx), Arc::new(ServerMiddlewareConfig::default()))
}

async fn call(app: &Router, method: &str, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request should be valid");
    let response = app.clone().oneshot(request).await.expect("router response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

#[tokio::test]
async fn analysis_stats_without_metrics_reports_unconfigured() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/analysis/stats").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true, "{body}");
    assert_eq!(body["data"]["configured"], false, "{body}");
}

#[tokio::test]
async fn analysis_llm_metrics_without_data_returns_a_success_envelope() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/analysis/llm-metrics").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true, "{body}");
}

#[tokio::test]
async fn agent_decision_graph_returns_an_empty_graph() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/agent-loops/missing/graph").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true, "{body}");
    assert!(body["data"].is_object(), "{body}");
}
