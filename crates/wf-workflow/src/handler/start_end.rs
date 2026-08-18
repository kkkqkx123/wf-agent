use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::error::ExecutionSharedResult;
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;
use crate::trigger_internal;

pub struct StartHandler;

#[async_trait]
impl NodeHandler for StartHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Start
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl StartHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let already_executed = ctx
            .get_variable(&trigger_internal::completed_marker(&ctx.node_id))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if already_executed {
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        }
        ctx.set_internal_variable(
            trigger_internal::completed_marker(&ctx.node_id),
            Value::from(true),
        );
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

pub struct EndHandler;

#[async_trait]
impl NodeHandler for EndHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::End
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl EndHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let already_executed = ctx
            .get_variable(&trigger_internal::completed_marker(&ctx.node_id))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if already_executed {
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        }
        ctx.set_internal_variable(
            trigger_internal::completed_marker(&ctx.node_id),
            Value::from(true),
        );
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}
