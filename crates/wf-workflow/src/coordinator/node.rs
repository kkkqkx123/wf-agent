use wf_execution_shared::context::NodeExecutionContext;
use wf_execution_shared::retry::budget::RetryBudget;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

pub struct NodeCoordinator;

impl NodeCoordinator {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute_node(
        &self,
        _handler: &dyn NodeHandler,
        _ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<crate::handler::NodeHandlerResult> {
        Ok(crate::handler::NodeHandlerResult::simple(serde_json::Value::Null))
    }

    pub async fn execute_with_retry(
        &self,
        _handler: &dyn NodeHandler,
        _ctx: &mut NodeExecutionContext,
        _retry_budget: &mut RetryBudget,
    ) -> WorkflowResult<crate::handler::NodeHandlerResult> {
        Ok(crate::handler::NodeHandlerResult::simple(serde_json::Value::Null))
    }
}
