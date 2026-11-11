use std::sync::Arc;

use wf_execution_shared::types::execution_entity::{
    ExecutionEntity, ExecutionStatus as EntityExecutionStatus,
};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, ParentLink, SpawnedAgentLoop};
use wf_types::Id;

use crate::coordinator::state_transitor::AgentLoopStateTransitor;
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

use super::reclaim::ChildRunReclaim;
use super::AgentLoopExecutor;

impl AgentLoopExecutor {
    /// Dispatch an agent loop in the background and return a handle
    /// immediately. The execution id is pre-generated and injected into the
    /// coordinator so the registry handle exists before the task runs:
    /// `query_execution_status` / `cancel_execution` work right away. The
    /// terminal output is stored in the registry result slot. When the
    /// caller is itself a registered agent loop (parent id in the context),
    /// the background task is cancelled through the parent's abort signal
    /// (trigger.rs pattern).
    pub async fn spawn_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<SpawnedAgentLoop> {
        self.spawn_with_parent(config, input, None).await
    }

    /// Background dispatch linked under a live parent execution. `None`
    /// runs as a root. The returned handle stays queryable even when the
    /// background build fails: the failure settles the placeholder as a
    /// terminal failed execution carrying the error instead of evaporating
    /// the id.
    pub async fn spawn_with_parent(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
        parent: Option<ParentLink>,
    ) -> AgentResult<SpawnedAgentLoop> {
        Self::gate_config(&config, &self.registry, self.max_iterations_cap)?;
        Self::gate_input_with_config(&config, &input)?;
        let config = Self::inherit_file_scope(config, parent.as_ref())?;
        let execution_id = Id::from(wf_common::generate_id());

        self.check_sub_agent_depth(parent.as_ref())?;

        // Pre-register a placeholder entity so the execution is queryable
        // and cancellable from the moment spawn returns; the coordinator
        // replaces it with the fully-built entity on its first steps. The
        // capacity gate is enforced here: overflow surfaces immediately.
        // The placeholder carries the parent linkage from the start, so a
        // caller polling in the window before the real entity registers
        // still resolves it inside its own execution subtree.
        let mut placeholder = AgentLoopEntity::new(execution_id.clone());
        if let Some(link) = parent.as_ref() {
            let child_manager = link
                .manager
                .derive_child(
                    execution_id.clone(),
                    wf_types::execution::ExecutionType::Agent,
                    None,
                )
                .map_err(|e| {
                    if e.is_hierarchy_depth_exceeded() {
                        crate::error::AgentError::HierarchyLimitReached(e.to_string())
                    } else {
                        crate::error::AgentError::Validation(e.to_string())
                    }
                })?;
            placeholder = placeholder.with_hierarchy_manager(child_manager);
        }
        if let Err(e) = self.agent_registry.register(Arc::new(placeholder)) {
            // The derive above registered the child on the parent manager;
            // a capacity-rejected spawn must not leave that reference behind.
            if let Some(link) = parent.as_ref() {
                link.manager.remove_child(
                    execution_id.as_str(),
                    &wf_types::execution::ExecutionType::Agent,
                );
            }
            return Err(e);
        }

        // Background dispatch reports its lifecycle to the parent stream
        // when observed: start now, finish when the background task
        // settles. The display name derives from the same config/input.
        let child_name = crate::subagent::display_name(&config, &input);
        if let Some(ref link) = parent {
            crate::subagent::emit_started(link, &execution_id, &config, &input, true);
        }
        let subagent_parent = parent.clone();
        let subagent_child_id = execution_id.clone();
        let subagent_child_name = child_name.clone();

        // Parent cancellation propagation: the typed link carries the live
        // parent abort signal, so a parent of any execution type stops the
        // child. A missing link means no cross-execution cancel.
        let parent_token = parent.as_ref().and_then(|link| link.cancellation.clone());

        let agent_registry = self.agent_registry.clone();
        let run_id = execution_id.clone();
        let coordinator = self
            .coordinator(parent.as_ref())
            .with_agent_loop_id(execution_id.clone());
        let event_bus = self.event_bus.clone();

        let handle = tokio::spawn(async move {
            let reclaim = ChildRunReclaim::arm(
                agent_registry.clone(),
                run_id.clone(),
                subagent_parent.as_ref().map(|link| link.manager.clone()),
            );
            let run = coordinator.execute(config, input).await;
            // Settled runs leave the parent hierarchy; the guard stays armed
            // only to reclaim the slot if this task is aborted mid-run.
            reclaim.finish();
            match run {
                Ok(output) => {
                    agent_registry.store_result(run_id.clone(), output);
                    if let Some(ref link) = subagent_parent {
                        crate::subagent::emit_ended(
                            link,
                            &subagent_child_id,
                            &subagent_child_name,
                            true,
                            true,
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        execution_id = %run_id,
                        error = %e,
                        "spawned agent loop failed"
                    );
                    if let Some(ref link) = subagent_parent {
                        crate::subagent::emit_ended(
                            link,
                            &subagent_child_id,
                            &subagent_child_name,
                            false,
                            true,
                        );
                    }
                    // Settle the placeholder as a terminal failed execution
                    // carrying the build error: the spawn handle keeps its
                    // query contract and the failure reason reaches polling
                    // callers through `query_execution_status` instead of
                    // surfacing as `NotFound`. An entity that already
                    // reached a terminal state keeps its own outcome.
                    if let Some(entity) = agent_registry.get(&run_id) {
                        let _ = entity.state.write().await.start();
                        let _ = AgentLoopStateTransitor::fail_agent_loop(
                            &entity,
                            e.to_string(),
                            event_bus.as_deref(),
                        )
                        .await;
                    }
                }
            }
            // Placeholder rollback: an execution that ended before the
            // coordinator replaced the spawn placeholder (registration
            // failure) leaves a Created entity behind; drop it so it
            // neither holds a capacity permit nor lingers in the registry.
            // A settled failed placeholder is history, not garbage: it
            // stays registered until `cleanup_terminated` removes it.
            if let Some(entity) = agent_registry.get(&run_id) {
                if matches!(entity.status(), EntityExecutionStatus::Created) {
                    agent_registry.unregister(&run_id);
                }
            }
            agent_registry.unregister_task(&run_id);
        });
        self.agent_registry
            .register_task(execution_id.clone(), handle);

        // Parent cancellation propagation: when the parent loop stops, the
        // child execution is stopped as well.
        if let Some(token) = parent_token {
            let agent_registry = self.agent_registry.clone();
            let child_id = execution_id.clone();
            tokio::spawn(async move {
                token.cancelled().await;
                if let Some(entity) = agent_registry.get(&child_id) {
                    let _ = entity.stop().await;
                }
            });
        }

        Ok(SpawnedAgentLoop {
            agent_loop_id: execution_id.clone(),
            execution_id,
            status: "started".to_string(),
        })
    }
}
