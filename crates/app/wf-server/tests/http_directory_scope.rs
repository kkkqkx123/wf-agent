//! Router-level directory scope and execution existence gate contracts.
//! The API contract tests use a router without a workspace scope, so these
//! cases exercise the middleware layers that decide which directory a
//! request may touch and which execution id may be read.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;
use wf_api::ApiContext;
use wf_resource::registry::ResourceRegistries;
use wf_server::directory::{DirectoryConfig, DirectoryContext};
use wf_server::middleware::ServerMiddlewareConfig;
use wf_storage::context::StorageContext;

fn app(dirs: Arc<DirectoryContext>) -> (Router, Arc<ApiContext>) {
    let ctx = Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ));
    let router = wf_server::api_router_with_dirs(
        Arc::clone(&ctx),
        Arc::new(ServerMiddlewareConfig::default()),
        dirs,
    );
    (router, ctx)
}

async fn call(
    router: &Router,
    method: &str,
    uri: &str,
    directory: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(directory) = directory {
        builder = builder.header("x-wf-directory", directory);
    }
    let response = router
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("router response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

#[tokio::test]
async fn directory_rejections_use_distinct_envelopes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("missing").to_string_lossy().into_owned();
    let dirs = Arc::new(DirectoryContext::new(DirectoryConfig::default()));
    let (router, _) = app(Arc::clone(&dirs));

    let (status, body) = call(&router, "GET", "/api/v1/workflows", Some(&missing)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "LOCATION_NOT_FOUND");

    dirs.set_ready(false);
    let (status, body) = call(&router, "GET", "/api/v1/workflows", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "SERVICE_UNAVAILABLE");
}

#[tokio::test]
async fn untrusted_workspace_allows_reads_but_rejects_writes() {
    let allowed = tempfile::tempdir().expect("allowed workspace");
    let trusted = tempfile::tempdir().expect("trusted workspace");
    let allowed_path = std::fs::canonicalize(allowed.path()).expect("canonical allowed");
    let trusted_path = std::fs::canonicalize(trusted.path()).expect("canonical trusted");
    let config = DirectoryConfig {
        allowed_roots: vec![allowed_path.clone(), trusted_path],
        // Non-empty and excluding the allowed workspace makes it untrusted.
        trusted_roots: vec![trusted.path().to_path_buf()],
    };
    let router = app(Arc::new(DirectoryContext::new(config))).0;
    let directory = allowed_path.to_string_lossy().into_owned();

    let (status, body) = call(&router, "GET", "/api/v1/workflows", Some(&directory)).await;
    assert_ne!(status, StatusCode::FORBIDDEN, "reads stay allowed: {body}");

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/git/action")
        .header("x-wf-directory", &directory)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"action":"status"}"#))
        .expect("request");
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("router response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["success"], false, "{body}");
}

#[tokio::test]
async fn directory_header_takes_precedence_over_query_parameter() {
    let workspace = tempfile::tempdir().expect("workspace");
    let valid = std::fs::canonicalize(workspace.path()).expect("canonical workspace");
    let router = app(Arc::new(DirectoryContext::new(DirectoryConfig::default()))).0;
    let valid_str = valid.to_string_lossy().into_owned();
    let missing_str = valid.join("does-not-exist").to_string_lossy().into_owned();

    // Header names a missing location while the query carries a valid one;
    // the header channel must win and yield LOCATION_NOT_FOUND.
    let uri = format!("/api/v1/workflows?directory={valid_str}");
    let request = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("x-wf-directory", &missing_str)
        .body(Body::empty())
        .expect("request");
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("router response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "LOCATION_NOT_FOUND", "{body}");
}

#[tokio::test]
async fn execution_view_gate_distinguishes_unknown_ids() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let work = std::fs::canonicalize(tmp.path()).expect("canonical workspace");
    let directory = work.to_string_lossy().into_owned();
    let (router, ctx) = app(Arc::new(DirectoryContext::new(DirectoryConfig::default())));

    let execution: wf_types::WorkflowExecution = serde_json::from_value(serde_json::json!({
        "id": "exec-scoped",
        "workflow_id": "wf-scoped",
        "status": "running",
        "started_at": 1,
        "directory": directory
    }))
    .expect("workflow execution record");
    wf_api::workflow::execution::save_execution(&ctx, &execution)
        .await
        .expect("save execution");

    let (status, _) = call(
        &router,
        "GET",
        "/api/v1/executions/exec-scoped/state",
        Some(&directory),
    )
    .await;
    assert_ne!(status, StatusCode::NOT_FOUND, "known id must pass the gate");

    let (status, body) = call(
        &router,
        "GET",
        "/api/v1/executions/exec-unknown/state",
        Some(&directory),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}
