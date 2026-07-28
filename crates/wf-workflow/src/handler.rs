pub mod agent_loop;
pub mod context_processor;
pub mod fork_join;
pub mod llm;
pub mod loop_handler;
pub mod route;
pub mod script;
pub mod start_end;
pub mod subgraph;
pub mod sync;
pub mod variable;

pub use wf_execution_shared::context::NodeExecutionResult as NodeHandlerResult;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;

#[async_trait]
pub trait NodeHandler: Send + Sync {
    fn node_type(&self) -> StaticNodeType;
    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeHandlerResult>;
}

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

    pub fn register_defaults(&mut self) {
        self.register(Arc::new(start_end::StartHandler));
        self.register(Arc::new(start_end::EndHandler));
        self.register(Arc::new(route::RouteHandler));
        self.register(Arc::new(variable::VariableHandler));
        self.register(Arc::new(loop_handler::LoopStartHandler));
        self.register(Arc::new(loop_handler::LoopEndHandler));
        self.register(Arc::new(fork_join::ForkHandler));
        self.register(Arc::new(fork_join::JoinHandler));
        self.register(Arc::new(sync::SyncHandler));
        self.register(Arc::new(subgraph::SubgraphHandler));
        self.register(Arc::new(llm::LlmHandler));
        self.register(Arc::new(context_processor::ContextProcessorHandler));
        self.register(Arc::new(script::ScriptHandler));
        self.register(Arc::new(agent_loop::AgentLoopHandler));
        self.register(Arc::new(start_end::ToolVisibilityHandler));
        self.register(Arc::new(start_end::EmbedHandler));
        self.register(Arc::new(start_end::UserInteractionHandler));
        self.register(Arc::new(start_end::TriggerPassthroughHandler));
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        let mut reg = Self::new();
        reg.register_defaults();
        reg
    }
}
