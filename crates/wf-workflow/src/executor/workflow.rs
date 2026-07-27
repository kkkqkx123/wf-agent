use crate::error::WorkflowResult;
use crate::handler::{NodeHandler, NodeHandlerResult};
use crate::graph::GraphTraversal;

pub struct WorkflowExecutor;

impl WorkflowExecutor {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute_workflow(
        &self,
        _graph: GraphTraversal,
        _handlers: &std::collections::HashMap<wf_types::node::StaticNodeType, std::sync::Arc<dyn NodeHandler>>,
    ) -> WorkflowResult<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }
}
