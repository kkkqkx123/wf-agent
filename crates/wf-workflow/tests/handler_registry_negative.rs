//! Negative tests for handler registry access (Phase 2 / B3): the registry
//! carried in the execution context is strongly typed (`NodeHandlerRegistry`,
//! no `Any` downcast), so a missing registry is the only runtime failure
//! mode and must surface as a structured error instead of silently degrading
//! to an empty registry. A "wrong registry type" cannot exist anymore — the
//! compiler rejects it.

use std::sync::Arc;

use dashmap::DashMap;
use serde_json::json;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::node::StaticNodeType;
use wf_workflow::handler::fork_join::ForkHandler;
use wf_workflow::handler::NodeHandler;

fn base_ctx() -> NodeExecutionContext {
    NodeExecutionContext::new(
        wf_types::Id::new(),
        "fork-1".to_string(),
        StaticNodeType::Fork,
        json!({}),
        Arc::new(DashMap::new()),
    )
    .with_node_config(json!({
        "fork_paths": [
            { "path_id": "p1", "child_node_id": "n1" },
            { "path_id": "p2", "child_node_id": "n2" },
        ]
    }))
}

#[tokio::test]
async fn fork_with_missing_registry_returns_structured_error() {
    let mut ctx = base_ctx();
    ctx.handler_registry = None;

    let result = ForkHandler.execute(&mut ctx).await;
    let err = result
        .err()
        .expect("fork must fail without a handler registry");
    assert!(
        err.to_string().contains("no handler registry"),
        "unexpected error message: {err}"
    );
}
