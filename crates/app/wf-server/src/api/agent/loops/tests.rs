//! Tests for the agent loop API surface.

use std::sync::Arc;

use axum::body::Body as AxBody;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use tower::ServiceExt;
use wf_api::ApiContext;
use wf_resource::registry::{register_item_skip, ResourceRegistries};

use super::lifecycle::{params_from_body, RunAgentLoopBody};

/// The built-in main agent template has to be registered, otherwise the
/// composition boundary rejects the run as an unknown agent.
const MAIN_AGENT_ID: &str = "@standard/main";

fn registries() -> ResourceRegistries {
    let regs = ResourceRegistries::new();
    register_item_skip(
        &regs.agent_templates,
        MAIN_AGENT_ID.into(),
        wf_resource::predefined::agent_templates::main_agent_template(),
    );
    wf_resource::predefined::agent_prompts::register(
        &regs,
        &wf_resource::registry::RegisterOptions::default(),
    );
    regs
}

fn make_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        wf_storage::context::StorageContext::new_memory(),
        Arc::new(registries()),
    ))
}

fn test_state() -> crate::router::ApiState {
    crate::router::ApiState {
        ctx: make_ctx(),
        config: Arc::new(crate::middleware::ServerMiddlewareConfig::default()),
        locks: crate::api::workflow::locks::DefinitionLocks::default(),
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
        "agent_id": MAIN_AGENT_ID,
        "model": "mock",
        "message": "hi",
        "checkpoint_message_interval": 5,
    }))
    .unwrap();
    assert_eq!(
        params_from_body(&test_state(), body)
            .await
            .expect("resolve params")
            .config
            .checkpoint_message_interval,
        Some(5)
    );

    let zero = serde_json::from_value::<RunAgentLoopBody>(serde_json::json!({
        "agent_id": MAIN_AGENT_ID,
        "model": "mock",
        "message": "hi",
        "checkpoint_message_interval": 0,
    }))
    .unwrap();
    assert_eq!(
        params_from_body(&test_state(), zero)
            .await
            .expect("resolve params")
            .config
            .checkpoint_message_interval,
        None
    );

    let absent = serde_json::from_value::<RunAgentLoopBody>(serde_json::json!({
        "agent_id": MAIN_AGENT_ID,
        "model": "mock",
        "message": "hi",
    }))
    .unwrap();
    assert_eq!(
        params_from_body(&test_state(), absent)
            .await
            .expect("resolve params")
            .config
            .checkpoint_message_interval,
        None
    );
}

#[tokio::test]
async fn run_body_rejects_empty_message_and_defaults_empty_model() {
    // Rule A1: an empty message is a hard error on every entry path.
    let empty = serde_json::from_value::<RunAgentLoopBody>(serde_json::json!({
        "agent_id": MAIN_AGENT_ID,
        "model": "mock",
        "message": "   ",
    }))
    .unwrap();
    assert!(params_from_body(&test_state(), empty).await.is_err());

    // Rule A2: an empty or sentinel model normalizes to the server default.
    for model in ["", "default"] {
        let body = serde_json::from_value::<RunAgentLoopBody>(serde_json::json!({
            "agent_id": MAIN_AGENT_ID,
            "model": model,
            "message": "hi",
        }))
        .unwrap();
        assert_eq!(
            params_from_body(&test_state(), body)
                .await
                .expect("resolve params")
                .config
                .model,
            wf_api::DEFAULT_MODEL
        );
    }
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
