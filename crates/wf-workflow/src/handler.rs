pub mod agent_loop;
pub mod context_processor;
pub mod embed;
pub mod fork_join;
pub mod interactive_script;
pub mod llm;
pub mod loop_handler;
pub mod output_mapping;
pub mod route;
pub mod script;
pub mod start_end;
pub mod subgraph;
pub mod sync;
pub mod tool_visibility;
pub mod trigger;
pub mod user_interaction;
pub mod variable;
pub mod variable_mapping;

pub use wf_execution_shared::context::NodeExecutionResult as NodeHandlerResult;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::context::NodeExecutionContext;
use wf_llm::LlmGateway;
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
    // No `Default`: a usable registry requires an injected LLM gateway
    // (`register_defaults`), an empty registry would silently lack LLM
    // handlers.
    #[allow(clippy::new_without_default)]
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

    /// Register the standard handler set. The LLM gateway is injected here
    /// and shared by the LLM and AGENT_LOOP handlers; script handlers fall
    /// back to their own default sandbox runtime.
    pub fn register_defaults(&mut self, gateway: Arc<LlmGateway>) {
        self.register_defaults_with_sandbox(gateway, None)
    }

    /// Register the standard handler set with a caller-provided shared
    /// sandbox runtime (profiles + global routing rules precompiled).
    pub fn register_defaults_with_sandbox(
        &mut self,
        gateway: Arc<LlmGateway>,
        sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
    ) {
        self.register(Arc::new(start_end::StartHandler));
        self.register(Arc::new(start_end::EndHandler));
        self.register(Arc::new(route::RouteHandler));
        self.register(Arc::new(variable::VariableHandler));
        self.register(Arc::new(loop_handler::LoopStartHandler));
        self.register(Arc::new(loop_handler::LoopEndHandler));
        self.register(Arc::new(fork_join::ForkHandler));
        self.register(Arc::new(fork_join::JoinHandler));
        self.register(Arc::new(sync::SyncHandler::new()));
        self.register(Arc::new(subgraph::SubgraphHandler));
        self.register(Arc::new(llm::LlmHandler::new(gateway.clone())));
        self.register(Arc::new(context_processor::ContextProcessorHandler));
        self.register(Arc::new(script::ScriptHandler::with_sandbox_opt(
            sandbox.clone(),
        )));
        self.register(Arc::new(
            interactive_script::InteractiveScriptHandler::with_sandbox_opt(sandbox),
        ));
        self.register(Arc::new(agent_loop::AgentLoopHandler::new(gateway)));
        self.register(Arc::new(tool_visibility::ToolVisibilityHandler));
        self.register(Arc::new(embed::EmbedHandler));
        self.register(Arc::new(user_interaction::UserInteractionHandler));
        self.register(Arc::new(trigger::StartFromTriggerHandler));
        self.register(Arc::new(trigger::ContinueFromTriggerHandler));
    }
}
