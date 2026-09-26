use std::collections::HashMap;

use serde_json::Value;
use wf_common::now;
use wf_core::interruption::InterruptionSignal;
use wf_core::EventBus;
use wf_types::events::{BaseEvent, EventType};

use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::persistence::build_workflow_execution;

use super::WorkflowCoordinator;

impl WorkflowCoordinator {
    /// Fire a workflow-scope lifecycle hook (WORKFLOW_BEFORE / WORKFLOW_AFTER)
    /// once around the whole execution. The hook pipeline is event-only:
    /// condition failures only degrade to a skipped event, never to an
    /// execution error.
    pub(super) async fn execute_workflow_scope_hook(&self, hook_type: &str) {
        let Some(entity) = self.entity.as_ref() else {
            return;
        };
        crate::hook::WorkflowHookEmitter::fire_workflow_point(
            entity,
            &self.hooks,
            hook_type,
            HashMap::new(),
            self.ctx.hook_handler_registry.as_deref(),
            self.ctx.event_bus.as_deref(),
        )
        .await;
        crate::hook::WorkflowHookEmitter::maybe_hook_checkpoint(
            &self.hooks,
            hook_type,
            self.checkpoint.as_ref(),
            entity,
        )
        .await;
    }

    /// Persist the start record (status is whatever the entity currently
    /// holds, normally `Running`).
    pub(super) async fn persist_start(&self) {
        let (Some(entity), Some(manager)) = (self.entity.as_ref(), self.state_manager.as_ref())
        else {
            return;
        };
        let record =
            build_workflow_execution(entity, self.traversal.graph(), &self.ctx.options, None).await;
        manager.persist_workflow(&record).await;
    }

    /// Persist the final record after the run reaches a terminal state. When
    /// the run errored without a terminal status (e.g. node failure), the
    /// entity state is marked failed first so the record reflects reality.
    pub(super) async fn persist_final(&self, output: Option<&Value>) {
        let (Some(entity), Some(manager)) = (self.entity.as_ref(), self.state_manager.as_ref())
        else {
            return;
        };

        let terminal = {
            let state = entity.state.read().await;
            matches!(
                state.status(),
                wf_execution_shared::types::execution_entity::ExecutionStatus::Completed
                    | wf_execution_shared::types::execution_entity::ExecutionStatus::Failed
                    | wf_execution_shared::types::execution_entity::ExecutionStatus::Cancelled
                    | wf_execution_shared::types::execution_entity::ExecutionStatus::Stopped
                    | wf_execution_shared::types::execution_entity::ExecutionStatus::Paused
            )
        };
        if !terminal {
            let state_transition = match entity.interruption().check() {
                Some(InterruptionSignal::Stop) => entity.state.write().await.cancel(),
                Some(InterruptionSignal::Pause) => entity.state.write().await.pause(),
                _ => entity
                    .state
                    .write()
                    .await
                    .fail("workflow execution failed".to_string()),
            };
            if let Err(e) = state_transition {
                tracing::debug!(
                    execution_id = %entity.id(),
                    error = %e,
                    "persist_final skipped terminal state transition"
                );
            }
        }

        let record = build_workflow_execution(
            entity,
            self.traversal.graph(),
            &self.ctx.options,
            output.cloned(),
        )
        .await;
        manager.persist_workflow(&record).await;
    }

    /// Abort the run when the execution is interrupted (Stopped/Paused) or
    /// exceeds its wall-clock `max_execution_time`. Emits the matching event
    /// and marks the entity state; returns `Err` to stop the main loop.
    pub(super) async fn check_interruption_and_timeout(
        &mut self,
        entity: &WorkflowExecutionEntity,
        event_bus: Option<&EventBus>,
        node_id: &str,
    ) -> WorkflowResult<()> {
        let interruption_check = wf_execution_shared::interruption::check_execution_interruption(
            entity.interruption(),
            None,
        );
        match interruption_check {
            wf_execution_shared::types::interruption::ExecutionInterruptionCheckResult::Stopped { .. } => {
                entity
                    .state
                    .write()
                    .await
                    .record_interruption(serde_json::json!({
                        "type": "stop",
                        "recovered": false,
                        "timestamp": now(),
                    }));
                self.emit_event(
                    event_bus,
                    EventType::WorkflowExecutionCancelled,
                    entity,
                    &serde_json::json!({ "reason": "interrupted" }),
                )
                .await;
                // Persist the cancelled state so the run can be audited and
                // resumed from storage instead of only living in memory.
                if let Some(ref mut cp) = self.checkpoint {
                    cp.on_interruption(entity).await;
                }
                return Err(WorkflowError::CoordinatorError(
                    "Execution stopped by interruption".to_string(),
                ));
            }
            wf_execution_shared::types::interruption::ExecutionInterruptionCheckResult::Paused { .. } => {
                entity
                    .state
                    .write()
                    .await
                    .record_interruption(serde_json::json!({
                        "type": "pause",
                        "recovered": true,
                        "timestamp": now(),
                    }));
                self.emit_event(
                    event_bus,
                    EventType::WorkflowExecutionPaused,
                    entity,
                    &serde_json::json!({ "node_id": node_id }),
                )
                .await;
                // The state is already `Paused`; snapshot it so a paused run
                // survives a crash and can be resumed from storage.
                if let Some(ref mut cp) = self.checkpoint {
                    cp.on_pause(entity).await;
                }
                return Err(WorkflowError::CoordinatorError(
                    "Execution paused".to_string(),
                ));
            }
            _ => {}
        }

        if let Some(max_execution_time) = self.ctx.options.max_execution_time {
            if max_execution_time > 0 && (now() - self.start_time) as u64 >= max_execution_time {
                tracing::warn!(
                    execution_id = %entity.id(),
                    max_execution_time,
                    "Workflow execution wall-clock timeout exceeded, stopping execution"
                );
                self.emit_event(
                    event_bus,
                    EventType::WorkflowExecutionCancelled,
                    entity,
                    &serde_json::json!({
                        "reason": "max_execution_time",
                        "max_execution_time": max_execution_time,
                    }),
                )
                .await;
                {
                    if let Some(ref metrics) = self.ctx.metrics {
                        metrics.timeout().record_expiration(
                            "workflow_wall_clock",
                            (now() - self.start_time) as f64,
                            &entity.id().to_string(),
                        );
                        metrics
                            .workflow()
                            .record_timeout(&entity.workflow_id().to_string());
                    }
                    let mut state = entity.state.write().await;
                    state.increment_timeout_count();
                    state.record_interruption(serde_json::json!({
                        "type": "timeout",
                        "reason": "max_execution_time",
                        "max_execution_time": max_execution_time,
                        "recovered": false,
                        "timestamp": now(),
                    }));
                    state.timeout("Workflow execution exceeded max_execution_time".to_string())?;
                }
                if let Some(ref mut cp) = self.checkpoint {
                    cp.on_timeout(entity).await;
                }
                return Err(WorkflowError::ExecutionTimeout(format!(
                    "Workflow execution exceeded max_execution_time ({}ms)",
                    max_execution_time
                )));
            }
        }
        Ok(())
    }

    pub(super) async fn emit_event(
        &self,
        event_bus: Option<&EventBus>,
        event_type: EventType,
        entity: &WorkflowExecutionEntity,
        data: &serde_json::Value,
    ) {
        let Some(bus) = event_bus else {
            tracing::debug!(
                execution_id = %entity.id(),
                ?event_type,
                "no event bus attached, skipping event emission"
            );
            return;
        };
        let metadata = data.as_object().map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<_, _>>()
        });
        // Lifecycle events are observability-critical: surface the loss at
        // error level. Execution itself is never aborted by a failed publish
        // (events are a side channel).
        let critical = matches!(
            event_type,
            EventType::WorkflowExecutionStarted
                | EventType::WorkflowExecutionCompleted
                | EventType::WorkflowExecutionFailed
                | EventType::WorkflowExecutionCancelled
        );
        let event_type_label = format!("{:?}", event_type);

        let event = BaseEvent {
            id: wf_types::Id::new(),
            r#type: event_type,
            timestamp: now(),
            event_name: None,
            workflow_id: Some(entity.workflow_id().clone()),
            execution_id: Some(entity.id().clone()),
            agent_loop_id: None,
            metadata,
        };
        match bus.publish_logged(
            event,
            &format!(
                "workflow={} node={}",
                entity.id(),
                self.current_node_id.as_deref().unwrap_or("")
            ),
        ) {
            Err(err) if critical => {
                tracing::error!(
                    execution_id = %entity.id(),
                    event_type = %event_type_label,
                    error = ?err,
                    "critical lifecycle event publish failed"
                );
            }
            _ => {}
        }
    }
}
