use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

pub struct StartHandler;

#[async_trait]
impl NodeHandler for StartHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Start
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let already_executed = ctx
            .get_variable(&format!("__completed_{}", ctx.node_id))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if already_executed {
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        }
        ctx.set_variable(format!("__completed_{}", ctx.node_id), Value::from(true));
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

pub struct EndHandler;

#[async_trait]
impl NodeHandler for EndHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::End
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let already_executed = ctx
            .get_variable(&format!("__completed_{}", ctx.node_id))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if already_executed {
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        }
        ctx.set_variable(format!("__completed_{}", ctx.node_id), Value::from(true));
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

pub struct TriggerPassthroughHandler;

#[async_trait]
impl NodeHandler for TriggerPassthroughHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::StartFromTrigger
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

pub struct ContinueFromTriggerHandler;

#[async_trait]
impl NodeHandler for ContinueFromTriggerHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::ContinueFromTrigger
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}
