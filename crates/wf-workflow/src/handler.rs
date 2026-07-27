use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;

#[async_trait]
pub trait NodeHandler: Send + Sync {
    fn node_type(&self) -> StaticNodeType;
    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult>;
}

pub use wf_execution_shared::context::NodeExecutionResult as NodeHandlerResult;

pub struct HandlerRegistry {
    handlers: HashMap<StaticNodeType, Arc<dyn NodeHandler>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register(&mut self, handler: Arc<dyn NodeHandler>) {
        self.handlers.insert(handler.node_type(), handler);
    }

    pub fn get(&self, node_type: &StaticNodeType) -> Option<Arc<dyn NodeHandler>> {
        self.handlers.get(node_type).cloned()
    }

    pub fn has_handler(&self, node_type: &StaticNodeType) -> bool {
        self.handlers.contains_key(node_type)
    }

    pub fn into_arc(self) -> Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>> {
        Arc::new(self.handlers)
    }
}
