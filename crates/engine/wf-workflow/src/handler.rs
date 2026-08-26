pub mod agent_loop;
pub mod context_processor;
pub mod embed_boundary;
pub mod fork_join;
pub mod interactive_script;
pub mod llm;
pub mod loop_handler;
pub mod message_node;
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
pub use wf_execution_shared::handler::NodeHandler;

use std::collections::HashMap;
use std::sync::Arc;

use wf_execution_shared::handler::NodeHandlerRegistry;
use wf_llm::LlmGateway;
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};

/// Resolve the shared handler registry from a node execution context.
///
/// The registry is strongly typed (see `wf_execution_shared::handler`); a
/// missing registry is the only failure mode and is surfaced as a structured
/// error instead of silently degrading.
pub(crate) fn resolve_handler_registry(
    ctx: &wf_execution_shared::context::NodeExecutionContext,
) -> WorkflowResult<Arc<NodeHandlerRegistry>> {
    ctx.handler_registry.clone().ok_or_else(|| {
        WorkflowError::CoordinatorError(
            "no handler registry available in node execution context".to_string(),
        )
    })
}

pub struct HandlerRegistry {
    handlers: HashMap<StaticNodeType, Box<dyn NodeHandler>>,
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

    pub fn register(&mut self, handler: Box<dyn NodeHandler>) {
        self.handlers.insert(handler.node_type(), handler);
    }

    pub fn get(&self, node_type: &StaticNodeType) -> Option<&dyn NodeHandler> {
        self.handlers.get(node_type).map(|h| h.as_ref())
    }

    pub fn has_handler(&self, node_type: &StaticNodeType) -> bool {
        self.handlers.contains_key(node_type)
    }

    pub fn into_arc(self) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
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
        self.register_defaults_with_capture(gateway, sandbox, None)
    }

    /// Like [`Self::register_defaults_with_sandbox`], additionally attaching
    /// the file-checkpoint manager to the script handlers so script
    /// executions diff their workspace scope and record changes on the
    /// executing actor partition.
    pub fn register_defaults_with_file_checkpoint(
        &mut self,
        gateway: Arc<LlmGateway>,
        sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
        file_checkpoint: Option<wf_checkpoint::file::FileCheckpointManager>,
    ) {
        self.register_defaults_with_capture(gateway, sandbox, file_checkpoint)
    }

    fn register_defaults_with_capture(
        &mut self,
        gateway: Arc<LlmGateway>,
        sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
        file_checkpoint: Option<wf_checkpoint::file::FileCheckpointManager>,
    ) {
        self.register(Box::new(start_end::StartHandler));
        self.register(Box::new(start_end::EndHandler));
        self.register(Box::new(route::RouteHandler));
        self.register(Box::new(variable::VariableHandler));
        self.register(Box::new(loop_handler::LoopStartHandler));
        self.register(Box::new(loop_handler::LoopEndHandler));
        self.register(Box::new(fork_join::ForkHandler));
        self.register(Box::new(fork_join::JoinHandler));
        self.register(Box::new(sync::SyncHandler::new()));
        self.register(Box::new(subgraph::SubgraphHandler));
        self.register(Box::new(llm::LlmHandler::new(gateway.clone())));
        self.register(Box::new(context_processor::ContextProcessorHandler));
        self.register(Box::new(
            script::ScriptHandler::with_sandbox_opt(sandbox.clone())
                .with_file_checkpoint_opt(file_checkpoint),
        ));
        self.register(Box::new(
            interactive_script::InteractiveScriptHandler::with_sandbox_opt(sandbox),
        ));
        self.register(Box::new(agent_loop::AgentLoopHandler::new(gateway)));
        self.register(Box::new(tool_visibility::ToolVisibilityHandler));
        self.register(Box::new(embed_boundary::EmbedStartHandler));
        self.register(Box::new(embed_boundary::EmbedEndHandler));
        self.register(Box::new(user_interaction::UserInteractionHandler));
        self.register(Box::new(message_node::StartFromMessageHandler));
        self.register(Box::new(message_node::ContinueFromMessageHandler));
    }
}
