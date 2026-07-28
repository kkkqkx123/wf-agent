use async_trait::async_trait;
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
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

pub struct ToolVisibilityHandler;

#[async_trait]
impl NodeHandler for ToolVisibilityHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::ToolVisibility
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

pub struct EmbedHandler;

#[async_trait]
impl NodeHandler for EmbedHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::EmbedGraph
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

pub struct UserInteractionHandler;

#[async_trait]
impl NodeHandler for UserInteractionHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::UserInteraction
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
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
