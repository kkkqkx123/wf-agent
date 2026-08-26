use std::collections::HashMap;
use std::sync::Arc;

use wf_core::EventBus;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_llm::LlmGateway;
use wf_sandbox::SandboxRuntime;
use wf_tools::callback::WorkflowOutput;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

use crate::coordinator::{WorkflowExecutionParams, WorkflowLifecycleCoordinator};
use crate::error::WorkflowResult;
use crate::handler::{HandlerRegistry, NodeHandler};

pub struct WorkflowExecutor {
    event_bus: Option<Arc<EventBus>>,
    gateway: Arc<LlmGateway>,
    /// Shared sandbox runtime (global profiles + routing rules); injected
    /// into the script handlers of executions started here. `None` uses
    /// per-handler defaults.
    sandbox: Option<Arc<SandboxRuntime>>,
}

impl Default for WorkflowExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowExecutor {
    pub fn new() -> Self {
        Self {
            event_bus: None,
            gateway: Arc::new(LlmGateway::new()),
            sandbox: None,
        }
    }

    pub fn with_gateway(gateway: Arc<LlmGateway>) -> Self {
        Self {
            event_bus: None,
            gateway,
            sandbox: None,
        }
    }

    pub fn with_event_bus(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus: Some(event_bus),
            gateway: Arc::new(LlmGateway::new()),
            sandbox: None,
        }
    }

    pub fn new_default() -> Self {
        Self {
            event_bus: Some(Arc::new(EventBus::new(1024))),
            gateway: Arc::new(LlmGateway::new()),
            sandbox: None,
        }
    }

    /// Inject a shared sandbox runtime (compiled global profiles + routing
    /// rules) into the script handlers of executions started here.
    pub fn with_sandbox(mut self, sandbox: Arc<SandboxRuntime>) -> Self {
        self.sandbox = Some(sandbox);
        self
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_workflow(
        &self,
        workflow_id: wf_types::Id,
        graph: WorkflowGraphStructure,
        options: WorkflowExecutionOptions,
        tool_registry: Arc<wf_tools::registry::ToolRegistry>,
        handlers: Option<Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>>,
        hooks: Vec<BaseHookDefinition>,
        resource_registries: Option<Arc<wf_resource::registry::ResourceRegistries>>,
    ) -> WorkflowResult<WorkflowOutput> {
        let handlers = handlers.unwrap_or_else(|| {
            let mut registry = HandlerRegistry::new();
            registry.register_defaults_with_sandbox(self.gateway.clone(), self.sandbox.clone());
            registry.into_arc()
        });

        crate::registry::register_graph(&workflow_id.to_string(), graph.clone());

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::new(),
            workflow_id,
            graph,
            options,
            handlers,
            tool_registry,
            resource_registries,
            input: None,
            hooks,
        };

        let lifecycle = WorkflowLifecycleCoordinator::new(self.event_bus.clone());
        lifecycle.execute_workflow(params).await
    }
}
