use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::json;
use wf_execution_shared::{
    ExecutionSharedError, NodeExecutionContext, NodeExecutionResult, NodeHandler,
    NodeHandlerRegistry,
};
use wf_types::node::StaticNodeType;

struct EchoHandler;

#[async_trait]
impl NodeHandler for EchoHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Variable
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> Result<NodeExecutionResult, ExecutionSharedError> {
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

struct FailHandler;

#[async_trait]
impl NodeHandler for FailHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        _ctx: &mut NodeExecutionContext,
    ) -> Result<NodeExecutionResult, ExecutionSharedError> {
        Err(ExecutionSharedError::HandlerError(
            "script boom".to_string(),
        ))
    }
}

fn node_context() -> NodeExecutionContext {
    NodeExecutionContext::new(
        "exec-1".to_string(),
        "node-1".to_string(),
        StaticNodeType::Variable,
        json!({"hello": "world"}),
        Arc::new(DashMap::new()),
    )
}

fn registry() -> NodeHandlerRegistry {
    let mut registry: NodeHandlerRegistry = std::collections::HashMap::new();
    registry.insert(StaticNodeType::Variable, Box::new(EchoHandler));
    registry.insert(StaticNodeType::Script, Box::new(FailHandler));
    registry
}

#[tokio::test]
async fn registry_dispatches_handler_by_node_type() {
    let registry = registry();
    let handler = registry
        .get(&StaticNodeType::Variable)
        .expect("handler found");
    assert_eq!(handler.node_type(), StaticNodeType::Variable);
    let mut ctx = node_context();
    let result = handler.execute(&mut ctx).await.expect("echo succeeds");
    assert_eq!(result.output, json!({"hello": "world"}));
}

#[tokio::test]
async fn registry_handler_error_propagates() {
    let registry = registry();
    let handler = registry
        .get(&StaticNodeType::Script)
        .expect("handler found");
    let mut ctx = node_context();
    let err = match handler.execute(&mut ctx).await {
        Ok(_) => panic!("handler failure propagates"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("script boom"));
}

#[test]
fn registry_missing_type_resolves_to_none() {
    let registry = registry();
    assert!(registry.get(&StaticNodeType::Llm).is_none());
}
