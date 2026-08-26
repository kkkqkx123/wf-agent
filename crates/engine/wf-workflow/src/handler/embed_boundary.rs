use async_trait::async_trait;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::error::ExecutionSharedResult;
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

/// Passthrough handler for the EMBED_START boundary node produced by the
/// EMBED_GRAPH preprocessing expansion (the embedded graph's START node).
/// Pure pass-through: the input flows into the embedded body unchanged.
pub struct EmbedStartHandler;

#[async_trait]
impl NodeHandler for EmbedStartHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::EmbedStart
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl EmbedStartHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

/// Passthrough handler for the EMBED_END boundary node produced by the
/// EMBED_GRAPH preprocessing expansion (the embedded graph's END node).
/// Pure pass-through: the embedded body's output flows downstream unchanged.
pub struct EmbedEndHandler;

#[async_trait]
impl NodeHandler for EmbedEndHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::EmbedEnd
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl EmbedEndHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}
