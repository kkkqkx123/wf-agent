use std::collections::HashMap;
use std::sync::Arc;

use wf_tools::callback::WorkflowOutput;

use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::hooks::types::HookDefinition;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::coordinator::WorkflowCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

use super::WorkflowLifecycleCoordinator;

impl WorkflowLifecycleCoordinator {
    /// Shared wiring for both resume entry points: restore the newest
    /// checkpoint snapshot and rebuild the coordinator over it. Returns the
    /// coordinator ready to `execute`, plus the restored snapshot id.
    pub(super) async fn build_resumed_coordinator(
        &self,
        execution_id: &str,
        workflow_id: wf_types::Id,
        graph: WorkflowGraphStructure,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
        tool_registry: Arc<wf_tools::registry::ToolRegistry>,
        hooks: Vec<HookDefinition>,
    ) -> WorkflowResult<(WorkflowCoordinator, String)> {
        use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;
        use wf_checkpoint::coordinator::CheckpointCoordinator;
        use wf_checkpoint::state::CheckpointStateManager;
        use wf_checkpoint::state::WorkflowCheckpointStateManager;

        let state_manager = WorkflowCheckpointStateManager::new(self.store.clone());
        let mut cp_coordinator = WorkflowCheckpointCoordinator::new(state_manager);
        if let Some(ref manager) = self.file_checkpoint_manager {
            cp_coordinator = cp_coordinator.with_file_checkpoint_manager(manager.clone());
        }

        let metadata = cp_coordinator
            .state_manager()
            .get_latest(execution_id)
            .await
            .map_err(|e| {
                crate::error::WorkflowError::CoordinatorError(format!(
                    "checkpoint query failed: {}",
                    e
                ))
            })?
            .ok_or_else(|| {
                crate::error::WorkflowError::CoordinatorError(format!(
                    "no checkpoint found for execution {}",
                    execution_id
                ))
            })?;

        let restored = cp_coordinator.restore(&metadata.id).await.map_err(|e| {
            crate::error::WorkflowError::CoordinatorError(format!(
                "checkpoint restore failed: {}",
                e
            ))
        })?;
        let snapshot = restored.snapshot;

        let entity = WorkflowExecutionEntity::new(
            wf_types::Id::from(snapshot.execution_id.clone()),
            workflow_id.clone(),
        );
        {
            let mut state = entity.state.write().await;
            state.start()?;
            for node_id in snapshot
                .node_results
                .as_ref()
                .map(|m| m.keys())
                .into_iter()
                .flatten()
            {
                state.mark_node_completed(node_id.clone());
            }
            // Carry the parked error-branch record into the fresh entity so
            // `restore_suspended_error_branch` can consume and rebuild it.
            if let Some(suspend) = snapshot.error_suspend.clone() {
                state.set_error_suspend(Some(suspend));
            }
        }

        let mut ctx = ExecutorContext::new(
            wf_types::Id::from(snapshot.execution_id.clone()),
            workflow_id,
            self.event_bus.clone(),
            tool_registry,
            wf_types::workflow_execution::WorkflowExecutionOptions {
                // The input lives in the restored "input" variable; without
                // it, restarted nodes would compute a Null input.
                input: snapshot.variable_state.variables.get("input").cloned(),
                max_steps: None,
                timeout: None,
                max_execution_time: None,
                // A resumed execution continues checkpointing; sub-workflow
                // resumes opt out explicitly.
                enable_checkpoints: Some(true),
                node_timeout: None,
                max_pause_duration: None,
                max_navigation_multiplier: None,
                loop_max_iterations_cap: None,
            },
        );
        ctx.variables = entity.variables().clone();
        for (name, value) in &snapshot.variable_state.variables {
            ctx.variables.insert(name.clone(), value.clone());
        }
        if let Some(ref bus) = self.signal_bus {
            ctx = ctx.with_signal_bus(bus.clone());
        }
        if let Some(ref metrics) = self.metrics {
            ctx = ctx.with_metrics(metrics.clone());
        }
        if let Some(ref registry) = self.hook_handler_registry {
            ctx = ctx.with_hook_handler_registry(registry.clone());
        }

        // A resumed execution continues checkpointing unless the options
        // explicitly disable it (sub-workflow resume semantics).
        let checkpoints_enabled = ctx.options.enable_checkpoints.unwrap_or(true);

        let mut coordinator = WorkflowCoordinator::new(ctx, graph, handlers)?
            .with_entity(entity)
            .with_hooks(hooks);
        coordinator.resume_from(&snapshot);
        // A checkpointed error suspend rebuilds its isolated scope (error
        // namespace over the frozen main path) so the resumed run continues
        // from the branch target exactly as a fresh suspend entry would.
        coordinator.restore_suspended_error_branch().await;

        // Same sub-workflow skip semantics as `execute`: checkpoints are
        // wired only when the (resumed) execution enables them.
        if checkpoints_enabled {
            if let Some(ref strategy) = self.checkpoint_strategy {
                let mut cp = crate::checkpoint::WorkflowCheckpointIntegration::new(
                    self.store.clone(),
                    strategy.clone(),
                );
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
                coordinator = coordinator.with_checkpoint(cp);
            }
        }

        Ok((coordinator, snapshot.execution_id))
    }

    /// Resume a workflow execution from its latest checkpoint.
    ///
    /// Loads the newest snapshot for `execution_id`, rebuilds the entity,
    /// variables and node outputs, then continues from the checkpointed
    /// node. Completed nodes are skipped by the coordinator.
    pub async fn resume_workflow(
        &self,
        execution_id: &str,
        workflow_id: wf_types::Id,
        graph: WorkflowGraphStructure,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
        tool_registry: Arc<wf_tools::registry::ToolRegistry>,
        hooks: Vec<HookDefinition>,
    ) -> WorkflowResult<WorkflowOutput> {
        let (mut coordinator, snapshot_execution_id) = self
            .build_resumed_coordinator(
                execution_id,
                workflow_id,
                graph,
                handlers,
                tool_registry,
                hooks,
            )
            .await?;

        let result = coordinator.execute().await;
        match result {
            Ok(output) => Ok(WorkflowOutput {
                execution_id: wf_types::Id::from(snapshot_execution_id),
                result: output,
            }),
            Err(e) => Err(e),
        }
    }

    /// External recovery entry for an execution parked at an error suspend
    /// point. Recovery continues from the checkpointed branch target (the
    /// failed node itself never re-runs); callers needing a re-run resend
    /// the execution explicitly. This is the trigger and management plane's
    /// handle for suspended error branches, reusing the standard checkpoint
    /// resume channel.
    pub async fn resume_suspended_error_branch(
        &self,
        execution_id: &str,
        workflow_id: wf_types::Id,
        graph: WorkflowGraphStructure,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
        tool_registry: Arc<wf_tools::registry::ToolRegistry>,
        hooks: Vec<HookDefinition>,
    ) -> WorkflowResult<WorkflowOutput> {
        self.resume_workflow(
            execution_id,
            workflow_id,
            graph,
            handlers,
            tool_registry,
            hooks,
        )
        .await
    }
}
