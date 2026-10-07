//! Tests for the agent loop API surface.

use std::sync::Arc;

use axum::body::Body as AxBody;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use tower::ServiceExt;
use wf_api::ApiContext;

use super::lifecycle::{params_from_body, RunAgentLoopBody};

fn make_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        wf_storage::context::StorageContext::new_memory(),
        Arc::new(wf_resource::registry::ResourceRegistries::new()),
    ))
}

fn test_state() -> crate::router::ApiState {
    crate::router::ApiState {
        ctx: make_ctx(),
        config: Arc::new(crate::middleware::ServerMiddlewareConfig::default()),
        locks: crate::api::workflow::locks::WorkflowLocks::default(),
    }
}

async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
    crate::router::api_router(ctx)
        .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn run_body_forwards_checkpoint_message_interval() {
    // REST backstop wiring: `checkpoint_message_interval` must reach
    // `AgentLoopConfig`; a zero value disables instead of passing
    // through (the engine treats `> 0` as enabled).
    // Async context required: `params_from_body` builds an `ApiContext`,
    // whose constructor spawns background tasks on the current runtime.
    let body = serde_json::from_value::<RunAgentLoopBody>(serde_json::json!({
        "agent_id": "agent1",
        "model": "mock",
        "message": "hi",
        "checkpoint_message_interval": 5,
    }))
    .unwrap();
    assert_eq!(
        params_from_body(&test_state(), body)
            .expect("resolve params")
            .config
            .checkpoint_message_interval,
        Some(5)
    );

    let zero = serde_json::from_value::<RunAgentLoopBody>(serde_json::json!({
        "agent_id": "agent1",
        "model": "mock",
        "message": "hi",
        "checkpoint_message_interval": 0,
    }))
    .unwrap();
    assert_eq!(
        params_from_body(&test_state(), zero)
            .expect("resolve params")
            .config
            .checkpoint_message_interval,
        None
    );

    let absent = serde_json::from_value::<RunAgentLoopBody>(serde_json::json!({
        "agent_id": "agent1",
        "model": "mock",
        "message": "hi",
    }))
    .unwrap();
    assert_eq!(
        params_from_body(&test_state(), absent)
            .expect("resolve params")
            .config
            .checkpoint_message_interval,
        None
    );
}

#[tokio::test]
async fn loop_registry_summaries_and_stats_are_reachable() {
    let ctx = make_ctx();
    for uri in [
        "/api/v1/agent-loops/summaries",
        "/api/v1/agent-loops/summaries?status=running",
        "/api/v1/agent-loops/summaries?profile_id=profile-1",
        "/api/v1/agent-loops/stats",
    ] {
        let response = send(ctx.clone(), uri).await;
        assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
    }
    // An unknown status is rejected.
    let invalid = send(ctx.clone(), "/api/v1/agent-loops/summaries?status=nope").await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
}
