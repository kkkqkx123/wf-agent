use async_trait::async_trait;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

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
