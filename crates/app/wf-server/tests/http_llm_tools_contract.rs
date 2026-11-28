//! Router-level contract tests for the LLM tool registry surface, consumed
//! by the web app's tool management screens. An empty registry yields an
//! empty page and a stats envelope; unknown tools map to not-found on both
//! the parameter-validation and execution paths.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
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

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let payload = body.map(|value| value.to_string()).unwrap_or_default();
    let request = builder
        .body(Body::from(payload))
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
async fn tool_list_returns_an_empty_page_with_envelope_fields() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/tools", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true, "{body}");
    assert!(body["data"].is_object(), "{body}");
    assert!(body["data"]["items"].is_array(), "{body}");
    assert!(body["data"].get("limit").is_some(), "{body}");
    assert!(body["data"].get("offset").is_some(), "{body}");
}

#[tokio::test]
async fn tool_stats_returns_a_success_envelope() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/tool-registry/stats", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true, "{body}");
    assert!(body["data"].is_object(), "{body}");
}

#[tokio::test]
async fn tool_validate_and_execute_report_unknown_tool_as_not_found() {
    let app = app();
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/tools/validate-params",
        Some(json!({"tool_id": "missing-tool", "parameters": {}})),
    )
    .await;
    assert_eq!(body["success"], false, "{body}");
    assert_eq!(body["error"]["code"], "NOT_FOUND", "{body}");
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/tools/execute",
        Some(json!({"tool_id": "missing-tool", "parameters": {}})),
    )
    .await;
    assert_eq!(body["success"], false, "{body}");
    assert_eq!(body["error"]["code"], "NOT_FOUND", "{body}");
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}
