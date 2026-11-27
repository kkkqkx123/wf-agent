//! Webhook ingress contract: per-hook authentication, execution routing and
//! the caller-supplied fire id are transport-level guarantees, so they are
//! asserted through the real router with an in-memory context.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use wf_api::ApiContext;
use wf_resource::registry::{register_item_skip, ResourceRegistries};
use wf_server::middleware::ServerMiddlewareConfig;
use wf_storage::context::StorageContext;

fn app() -> Router {
    let ctx = Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ));
    register_hook(
        &ctx,
        "secured",
        json!({
            "path": "/hooks/secured",
            "auth": {"mode": "token", "token": "hook-secret"},
            "target": {"kind": "create", "workflow_id": "wf-hook"}
        }),
    );
    register_hook(
        &ctx,
        "live",
        json!({
            "path": "/hooks/live",
            "auth": {"mode": "none"},
            "target": {"kind": "execution_scoped"}
        }),
    );
    wf_server::api_router_with_config(ctx, Arc::new(ServerMiddlewareConfig::default()))
}

fn register_hook(ctx: &ApiContext, name: &str, spec: Value) {
    let template: wf_types::trigger::TriggerTemplate = serde_json::from_value(json!({
        "name": name,
        "created_at": 1000,
        "updated_at": 1000,
        "metadata": {"webhook_spec": spec}
    }))
    .expect("webhook trigger template");
    register_item_skip(
        &ctx.registries.trigger_templates,
        name.to_string(),
        template,
    );
}

async fn call(
    router: &Router,
    name: &str,
    headers: &[(&str, &str)],
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/hooks/{name}"))
        .header("content-type", "application/json");
    for (key, value) in headers {
        builder = builder.header(*key, *value);
    }
    let response = router
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("router response");
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
async fn webhook_auth_rejects_unknown_and_unauthenticated_fires() {
    let router = app();
    let (status, body) = call(&router, "missing", &[], json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["success"], false);

    let (status, body) = call(&router, "secured", &[], json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["success"], false);

    let (status, _) = call(&router, "secured", &[("x-hook-token", "wrong")], json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn webhook_accepts_both_token_headers_and_echoes_the_fire_id() {
    let router = app();
    for headers in [
        vec![("x-hook-token", "hook-secret")],
        vec![("authorization", "Bearer hook-secret")],
    ] {
        let (status, body) = call(
            &router,
            "secured",
            &headers,
            json!({"fire_id": "retry-1", "payload": 7}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["success"], true);
        assert_eq!(body["data"]["fired"], true);
        assert_eq!(body["data"]["fire_id"], "retry-1");
        assert!(body["data"]["execution_id"].is_null());
    }
}

#[tokio::test]
async fn execution_scoped_webhook_requires_an_execution_id() {
    let router = app();
    let (status, body) = call(&router, "live", &[], json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["success"], false);

    let (status, body) = call(
        &router,
        "live",
        &[],
        json!({"execution_id": "exec-hook", "fire_id": "f1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["execution_id"], "exec-hook");
    assert_eq!(body["data"]["fire_id"], "f1");
}
