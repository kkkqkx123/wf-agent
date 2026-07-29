use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_checkpoint::event::CheckpointEventBus;
use wf_core::EventBus;
use wf_core::WorkflowStateMachine;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::hooks::executor::HookExecutor;
use wf_storage::backend::StorageBackend;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

use crate::checkpoint::{NodeCheckpointStrategy, WorkflowCheckpointIntegration};
use crate::coordinator::WorkflowCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct WorkflowExecutionParams {
    pub execution_id: wf_types::Id,
    pub workflow_id: wf_types::Id,
    pub graph: WorkflowGraphStructure,
    pub options: WorkflowExecutionOptions,
    pub handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    pub tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    pub input: Option<Value>,
}

pub struct WorkflowLifecycleCoordinator {
    event_bus: Option<Arc<EventBus>>,
    hook_executor: Option<Arc<HookExecutor>>,
    store: Arc<StorageBackend>,
    checkpoint_strategy: Option<NodeCheckpointStrategy>,
    checkpoint_event_bus: Option<CheckpointEventBus>,
}

impl WorkflowLifecycleCoordinator {
    pub fn new(event_bus: Option<Arc<EventBus>>) -> Self {
        Self::with_store(event_bus, Arc::new(StorageBackend::new_memory()))
    }

    pub fn with_store(event_bus: Option<Arc<EventBus>>, store: Arc<StorageBackend>) -> Self {
        Self {
            event_bus,
            hook_executor: None,
            store,
            checkpoint_strategy: None,
            checkpoint_event_bus: None,
        }
    }

    pub fn with_hook_executor(mut self, hook_executor: Arc<HookExecutor>) -> Self {
        self.hook_executor = Some(hook_executor);
        self
    }

    pub fn with_checkpoint_strategy(mut self, strategy: NodeCheckpointStrategy) -> Self {
        self.checkpoint_strategy = Some(strategy);
        self
    }

    pub fn with_checkpoint_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.checkpoint_event_bus = Some(bus);
        self
    }

    pub async fn execute_workflow(
        &self,
        params: WorkflowExecutionParams,
    ) -> WorkflowResult<Value> {
        let execution_id = params.execution_id;
        let workflow_id = params.workflow_id;

        let mut wf_state = WorkflowStateMachine::new(&execution_id);
        wf_state.start().map_err(|e| WorkflowError::StateTransitionError(e.to_string()))?;

        let mut opts = params.options;
        if opts.input.is_none() {
            opts.input = params.input;
        }

        let entity = WorkflowExecutionEntity::new(
            execution_id.clone(),
            workflow_id.clone(),
        );

        if let Some(ref input) = opts.input {
            entity.set_variable("input", input.clone());
        }

        let ctx = ExecutorContext::new(
            execution_id.clone(),
            workflow_id,
            self.event_bus.clone(),
            params.tool_registry,
            opts,
        );

        let mut coordinator = WorkflowCoordinator::new(ctx, params.graph, params.handlers)?
            .with_entity(entity)
            .with_hooks(Vec::new());

        if let Some(ref executor) = self.hook_executor {
            coordinator = coordinator.with_hook_executor(executor.clone());
        }

        if let Some(ref strategy) = self.checkpoint_strategy {
            let mut cp = WorkflowCheckpointIntegration::new(self.store.clone(), strategy.clone());
            if let Some(ref bus) = self.checkpoint_event_bus {
                cp = cp.with_event_bus(bus.clone());
            }
            if let Some(ref core_bus) = self.event_bus {
                cp = cp.with_core_event_bus(core_bus.clone());
            }
            coordinator = coordinator.with_checkpoint(cp);
        }

        let result = coordinator.execute().await;

        match &result {
            Ok(output) => {
                wf_state.complete(Some(output.clone())).map_err(|e| {
                    WorkflowError::StateTransitionError(e.to_string())
                })?;
            }
            Err(e) => {
                wf_state.fail(e.to_string()).map_err(|e| {
                    WorkflowError::StateTransitionError(e.to_string())
                })?;
            }
        }

        result
    }
}
