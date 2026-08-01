use std::collections::HashMap;
use std::sync::Arc;

use wf_core::EventBus;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_tools::callback::WorkflowOutput;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

use crate::coordinator::{WorkflowExecutionParams, WorkflowLifecycleCoordinator};
use crate::error::WorkflowResult;
use crate::handler::{HandlerRegistry, NodeHandler};

pub struct WorkflowExecutor {
    event_bus: Option<Arc<EventBus>>,
}

impl Default for WorkflowExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowExecutor {
    pub fn new() -> Self {
        Self { event_bus: None }
    }

    pub fn with_event_bus(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus: Some(event_bus),
        }
    }

    pub fn new_default() -> Self {
        Self {
            event_bus: Some(Arc::new(EventBus::new(1024))),
        }
    }

    pub async fn execute_workflow(
        &self,
        workflow_id: wf_types::Id,
        graph: WorkflowGraphStructure,
        options: WorkflowExecutionOptions,
        tool_registry: Arc<wf_tools::registry::ToolRegistry>,
        handlers: Option<Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>>,
        hooks: Vec<BaseHookDefinition>,
    ) -> WorkflowResult<WorkflowOutput> {
        let handlers = handlers.unwrap_or_else(|| {
            let mut registry = HandlerRegistry::new();
            registry.register_defaults();
            registry.into_arc()
        });

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::new(),
            workflow_id,
            graph,
            options,
            handlers,
            tool_registry,
            input: None,
            hooks,
        };

        let lifecycle = WorkflowLifecycleCoordinator::new(self.event_bus.clone());
        lifecycle.execute_workflow(params).await
    }
}
