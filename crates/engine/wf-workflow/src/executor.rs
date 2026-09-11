use std::collections::HashMap;
use std::sync::Arc;

use wf_core::internal_signal::InternalSignalBus;
use wf_core::EventBus;
use wf_execution_shared::hooks::types::HookDefinition;
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
    /// Typed signal bus for internal workflow/agent signals.
    signal_bus: Option<Arc<InternalSignalBus>>,
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

/// Everything a caller must supply to start one workflow execution through
/// [`WorkflowExecutor::execute_workflow`]. Grouping these into a single struct
/// keeps the entry point's signature small and the optional collaborators
/// (`handlers`, `resource_registries`, `hooks`) explicit at the call site.
pub struct WorkflowRunRequest {
    pub workflow_id: wf_types::Id,
    pub graph: WorkflowGraphStructure,
    pub options: WorkflowExecutionOptions,
    pub tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    pub handlers: Option<Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>>,
    pub hooks: Vec<HookDefinition>,
    pub resource_registries: Option<Arc<wf_resource::registry::ResourceRegistries>>,
}

impl WorkflowExecutor {
    pub fn new() -> Self {
        Self {
            event_bus: None,
            signal_bus: None,
            gateway: Arc::new(LlmGateway::new()),
            sandbox: None,
        }
    }

    pub fn with_gateway(gateway: Arc<LlmGateway>) -> Self {
        Self {
            event_bus: None,
            signal_bus: None,
            gateway,
            sandbox: None,
        }
    }

    pub fn with_event_bus(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus: Some(event_bus),
            signal_bus: None,
            gateway: Arc::new(LlmGateway::new()),
            sandbox: None,
        }
    }

    pub fn new_default() -> Self {
        Self {
            event_bus: Some(Arc::new(EventBus::new(1024))),
            signal_bus: None,
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

    /// Inject the typed signal bus: control signals from trigger actions
    /// reach the coordinator loop of executions started here.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_bus = Some(bus);
        self
    }

    pub async fn execute_workflow(
        &self,
        request: WorkflowRunRequest,
    ) -> WorkflowResult<WorkflowOutput> {
        let WorkflowRunRequest {
            workflow_id,
            graph,
            options,
            tool_registry,
            handlers,
            hooks,
            resource_registries,
        } = request;

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

        let mut lifecycle = WorkflowLifecycleCoordinator::new(self.event_bus.clone());
        if let Some(ref bus) = self.signal_bus {
            lifecycle = lifecycle.with_signal_bus(bus.clone());
        }
        lifecycle.execute_workflow(params).await
    }
}
