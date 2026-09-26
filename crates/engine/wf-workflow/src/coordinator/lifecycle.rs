use std::sync::Arc;

use wf_checkpoint::event::CheckpointEventBus;
use wf_checkpoint::execution_events::ExecutionEventBus;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::EventBus;
use wf_core::WorkflowStateMachine;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_metrics::MetricsRegistry;
use wf_storage::backend::StorageBackend;
use wf_tools::callback::WorkflowOutput;

use crate::checkpoint::{NodeCheckpointStrategy, WorkflowCheckpointIntegration};
use crate::coordinator::WorkflowCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::trigger::states::TriggerStateRegistry;

mod params;
mod resume;

pub use params::WorkflowExecutionParams;

pub struct WorkflowLifecycleCoordinator {
    event_bus: Option<Arc<EventBus>>,
    /// Typed signal bus for internal workflow/agent signals (replaces the
    /// `__`-prefixed variable protocol).
    signal_bus: Option<Arc<InternalSignalBus>>,
    store: Arc<StorageBackend>,
    checkpoint_strategy: Option<NodeCheckpointStrategy>,
    checkpoint_event_bus: Option<CheckpointEventBus>,
    checkpoint_execution_events: Option<ExecutionEventBus>,
    metrics: Option<Arc<MetricsRegistry>>,
    trigger_state_registry: Option<Arc<TriggerStateRegistry>>,
    /// Shared hook receiver registry; hook points dispatch through it.
    hook_handler_registry: Option<Arc<HookHandlerRegistry>>,
    /// Optional file checkpoint manager: file snapshots are created on
    /// checkpoint persistence and restored after workflow restore
    /// (best-effort).
    file_checkpoint_manager: Option<wf_checkpoint::file::FileCheckpointManager>,
}

impl WorkflowLifecycleCoordinator {
    pub fn new(event_bus: Option<Arc<EventBus>>) -> Self {
        Self::with_store(event_bus, Arc::new(StorageBackend::new_memory()))
    }

    pub fn with_store(event_bus: Option<Arc<EventBus>>, store: Arc<StorageBackend>) -> Self {
        Self {
            event_bus,
            signal_bus: None,
            store,
            checkpoint_strategy: None,
            checkpoint_event_bus: None,
            checkpoint_execution_events: None,
            metrics: None,
            trigger_state_registry: None,
            hook_handler_registry: None,
            file_checkpoint_manager: None,
        }
    }

    /// Attach the file checkpoint manager: file snapshots of executions are
    /// created/restored through it (best-effort).
    pub fn with_file_checkpoint_manager(
        mut self,
        manager: wf_checkpoint::file::FileCheckpointManager,
    ) -> Self {
        self.file_checkpoint_manager = Some(manager);
        self
    }

    /// Inject the typed signal bus: control signals from trigger actions
    /// reach the coordinator loop of executions started here.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_bus = Some(bus);
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

    /// Register the execution event bus; `state_changed` events are published
    /// after every checkpoint creation.
    pub fn with_checkpoint_execution_events(mut self, bus: ExecutionEventBus) -> Self {
        self.checkpoint_execution_events = Some(bus);
        self
    }

    /// Register the trigger runtime state registry; checkpoints of
    /// executions started here capture the `trigger_states` audit trail.
    pub fn with_trigger_state_registry(mut self, registry: Arc<TriggerStateRegistry>) -> Self {
        self.trigger_state_registry = Some(registry);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Inject the shared hook receiver registry: hook points and engine
    /// signals of executions started here dispatch through it.
    pub fn with_hook_handler_registry(mut self, registry: Arc<HookHandlerRegistry>) -> Self {
        self.hook_handler_registry = Some(registry);
        self
    }

    /// Build the checkpoint integration for a checkpoint-enabled execution,
    /// wired with every optional manager/bus/registry the lifecycle
    /// coordinator holds. Returns `None` when no strategy is configured.
    pub(super) fn build_checkpoint_integration(&self) -> Option<WorkflowCheckpointIntegration> {
        let strategy = self.checkpoint_strategy.as_ref()?;
        let mut cp = WorkflowCheckpointIntegration::new(self.store.clone(), strategy.clone());
        if let Some(ref manager) = self.file_checkpoint_manager {
            cp = cp.with_file_checkpoint_manager(manager.clone());
        }
        if let Some(ref registry) = self.trigger_state_registry {
            cp = cp.with_trigger_state_registry(registry.clone());
        }
        if let Some(ref bus) = self.checkpoint_event_bus {
            cp = cp.with_event_bus(bus.clone());
        }
        if let Some(ref core_bus) = self.event_bus {
            cp = cp.with_core_event_bus(core_bus.clone());
        }
        if let Some(ref bus) = self.checkpoint_execution_events {
            cp = cp.with_execution_event_bus(bus.clone());
        }
        Some(cp)
    }

    pub async fn execute_workflow(
        &self,
        params: WorkflowExecutionParams,
    ) -> WorkflowResult<WorkflowOutput> {
        let WorkflowExecutionParams {
            execution_id,
            workflow_id,
            graph,
            mut options,
            handlers,
            tool_registry,
            resource_registries,
            input,
            hooks,
        } = params;
        let workflow_id_metrics = workflow_id.clone();

        // Reject structurally invalid graphs before execution starts.
        let _validated =
            crate::validation::GraphValidator::validate(graph.clone()).map_err(|errors| {
                let detail = errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                WorkflowError::GraphError(format!(
                    "Workflow graph validation failed ({} error(s)): {}",
                    errors.len(),
                    detail
                ))
            })?;

        let mut wf_state = WorkflowStateMachine::new(&execution_id);
        wf_state
            .start()
            .map_err(|e| WorkflowError::StateTransitionError(e.to_string()))?;

        if options.input.is_none() {
            options.input = input;
        }

        let entity = WorkflowExecutionEntity::new(execution_id.clone(), workflow_id.clone());

        if let Some(ref input) = options.input {
            entity.set_variable("input", input.clone());
        }

        // Checkpoints are skipped unless explicitly enabled: sub-workflows
        // (SUBGRAPH/EMBED/trigger) pass `enable_checkpoints: Some(false)`.
        let checkpoints_enabled = options.enable_checkpoints.unwrap_or(true);

        let mut ctx = ExecutorContext::new(
            execution_id.clone(),
            workflow_id,
            self.event_bus.clone(),
            tool_registry,
            options,
        );
        // The coordinator and the entity share one variable map so that
        // checkpoints (built from the entity) capture live variables.
        ctx.variables = entity.variables().clone();
        if let Some(ref bus) = self.signal_bus {
            ctx = ctx.with_signal_bus(bus.clone());
        }
        if let Some(ref regs) = resource_registries {
            ctx = ctx.with_resource_registries(regs.clone());
        }
        if let Some(ref metrics) = self.metrics {
            metrics
                .workflow()
                .record_execution_start(&workflow_id_metrics);
            ctx = ctx.with_metrics(metrics.clone());
        }
        if let Some(ref registry) = self.hook_handler_registry {
            ctx = ctx.with_hook_handler_registry(registry.clone());
        }

        let mut coordinator = WorkflowCoordinator::new(ctx, graph, handlers)?
            .with_entity(entity)
            .with_hooks(hooks);

        // Checkpoints are skipped unless explicitly enabled: sub-workflows
        // (SUBGRAPH/EMBED/trigger) pass `enable_checkpoints: Some(false)`.
        if checkpoints_enabled {
            if let Some(cp) = self.build_checkpoint_integration() {
                coordinator = coordinator.with_checkpoint(cp);
            }
        }

        let start = wf_common::now();
        let result = coordinator.execute().await;
        let duration_ms = (wf_common::now() - start) as f64;

        match result {
            Ok(output) => {
                wf_state
                    .complete(Some(output.clone()))
                    .map_err(|e| WorkflowError::StateTransitionError(e.to_string()))?;
                if let Some(ref metrics) = self.metrics {
                    metrics.workflow().record_execution_complete(
                        &workflow_id_metrics,
                        None,
                        true,
                        duration_ms,
                        None,
                    );
                }

                Ok(WorkflowOutput {
                    execution_id,
                    result: output,
                })
            }
            Err(e) => {
                // Settle the failure only when the run has not already reached
                // a terminal state on its own (wall-clock timeout, stop,
                // cancel). A settle failure is logged, never propagated:
                // replacing the root error with a state-transition error
                // would hide what actually ended the run.
                if !wf_state.is_terminal() {
                    if let Err(settle_err) = wf_state.fail(e.to_string()) {
                        tracing::error!(
                            "failed to settle terminal state after workflow error '{e}': {settle_err}"
                        );
                    }
                }
                if let Some(ref metrics) = self.metrics {
                    metrics.workflow().record_execution_complete(
                        &workflow_id_metrics,
                        None,
                        false,
                        duration_ms,
                        Some("workflow_error"),
                    );
                }
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests;
