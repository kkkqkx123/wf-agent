//! Router-level contract tests for the system settings surface: retention
//! policy and code-context diagnostics, both used by the web app settings
//! pages. Retention reads a default policy from the memory context, writes
//! validate the configuration, and the dry-run scan reports its mode.

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
async fn retention_settings_read_returns_default_envelope() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/system/retention", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true, "{body}");
    assert!(body["data"].is_object(), "{body}");
}

#[tokio::test]
async fn retention_settings_rejects_invalid_configuration() {
    let app = app();
    let (status, body) = call(
        &app,
        "PUT",
        "/api/v1/system/retention",
        Some(json!({"enabled": "not-a-boolean"})),
    )
    .await;
    assert_eq!(body["success"], false, "{body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR", "{body}");
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn retention_scan_dry_run_returns_a_report() {
    let app = app();
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/system/retention/scan",
        Some(json!({"dry_run": true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true, "{body}");
    assert_eq!(body["data"]["dry_run"], true, "{body}");
}

#[tokio::test]
async fn code_context_diagnostic_returns_a_success_envelope() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/system/code-context", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true, "{body}");
}
