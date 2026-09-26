use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_common::now;
use wf_core::internal_signal::InternalSignal;
use wf_core::interruption::InterruptionSignal;
use wf_core::EventBus;
use wf_types::workflow::error_branch::ErrorSuspendState;

use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;
use crate::error_branch::{ErrorBranchScope, ErrorFailureAction};

use super::WorkflowCoordinator;

impl WorkflowCoordinator {
    /// Enter (or re-enter, on nested branch failures) the isolated error
    /// scope: freeze the main-path map, run the branch on an overlay cloned
    /// from the entry snapshot plus the read-only error namespace.
    pub(super) async fn enter_error_branch(
        &mut self,
        entity: &WorkflowExecutionEntity,
        event_bus: Option<&EventBus>,
        summary: wf_types::workflow::error_branch::ErrorBranchSummary,
        from_suspend: bool,
    ) {
        let event_type = if from_suspend {
            wf_types::events::EventType::WorkflowErrorBranchResumed
        } else {
            wf_types::events::EventType::WorkflowErrorBranchTaken
        };
        if let Some(scope) = self.error_scope.as_mut() {
            scope.summary = summary.clone();
            for (key, value) in summary.variables() {
                self.ctx.variables.insert(key, value);
            }
        } else {
            let main_variables = self.ctx.variables.clone();
            let snapshot: HashMap<String, Value> = main_variables
                .iter()
                .map(|entry| (entry.key().clone(), entry.value().clone()))
                .collect();
            let overlay = Arc::new(dashmap::DashMap::new());
            for (key, value) in &snapshot {
                overlay.insert(key.clone(), value.clone());
            }
            for (key, value) in summary.variables() {
                overlay.insert(key, value);
            }
            self.ctx.variables = overlay;
            self.error_scope = Some(ErrorBranchScope {
                summary: summary.clone(),
                main_variables,
                snapshot,
            });
        }
        self.emit_event(
            event_bus,
            event_type,
            entity,
            &serde_json::json!({
                "error_category": summary.category.as_str(),
                "error_message": summary.message,
                "source_node_id": summary.source_node_id,
                "attempts": summary.attempts,
            }),
        )
        .await;
    }

    /// Park the execution at an error suspend point: persist the typed
    /// recovery record into the execution state (checkpointed through the
    /// state snapshot's `error_suspend` domain, never the business variable
    /// map), point the resume cursor at the branch target, and end the run
    /// through the standard paused protocol so a crash still resumes from
    /// storage. The failed node itself never re-runs; recovery continues
    /// from the branch target.
    pub(super) async fn suspend_error_branch(
        &mut self,
        entity: &WorkflowExecutionEntity,
        event_bus: Option<&EventBus>,
        summary: wf_types::workflow::error_branch::ErrorBranchSummary,
        target_node_id: &str,
    ) -> ErrorFailureAction {
        entity
            .state
            .write()
            .await
            .set_error_suspend(Some(ErrorSuspendState {
                summary: summary.clone(),
                target_node_id: target_node_id.to_string(),
                suspended_at: now(),
            }));
        entity
            .state
            .write()
            .await
            .set_current_node(Some(target_node_id.to_string()));
        let _ = entity.interruption().pause();
        self.emit_event(
            event_bus,
            wf_types::events::EventType::WorkflowErrorBranchSuspended,
            entity,
            &serde_json::json!({
                "error_category": summary.category.as_str(),
                "error_message": summary.message,
                "source_node_id": summary.source_node_id,
                "target_node_id": target_node_id,
                "attempts": summary.attempts,
            }),
        )
        .await;
        match self
            .check_interruption_and_timeout(entity, event_bus, target_node_id)
            .await
        {
            Err(paused) => ErrorFailureAction::Suspended(paused),
            Ok(()) => {
                // The pause was cleared concurrently: fall back to continuing
                // on the isolated branch instead of losing the failure, and
                // consume the suspend record so it cannot resurrect on a
                // later checkpoint restore.
                entity.state.write().await.set_error_suspend(None);
                self.enter_error_branch(entity, event_bus, summary, false)
                    .await;
                self.current_node_id = Some(target_node_id.to_string());
                ErrorFailureAction::Continue
            }
        }
    }

    /// Explicit merge point: flush isolated branch writes back to the main
    /// path (last-writer-wins), audit the conflicting keys with names only,
    /// and reclaim the error namespace by restoring the main-path map.
    pub(super) async fn merge_error_scope(&mut self, entity: &WorkflowExecutionEntity) {
        let Some(scope) = self.error_scope.take() else {
            return;
        };
        let event_bus = self.ctx.event_bus.clone();
        let mut merged: Vec<String> = Vec::new();
        let mut conflicts: Vec<String> = Vec::new();
        for entry in self.ctx.variables.iter() {
            let key = entry.key().clone();
            if ErrorBranchScope::is_machinery_key(&key) {
                continue;
            }
            let value = entry.value().clone();
            match scope.snapshot.get(&key) {
                Some(before) if before == &value => {}
                Some(_) => {
                    scope.main_variables.insert(key.clone(), value);
                    merged.push(key.clone());
                    conflicts.push(key);
                }
                None => {
                    scope.main_variables.insert(key.clone(), value);
                    merged.push(key);
                }
            }
        }
        self.ctx.variables = scope.main_variables;
        if !merged.is_empty() {
            self.emit_event(
                event_bus.as_deref(),
                wf_types::events::EventType::VariableChanged,
                entity,
                &serde_json::json!({
                    "merged_keys": merged,
                    "conflicts": conflicts,
                    "source": "error_branch_merge",
                }),
            )
            .await;
        }
    }

    /// Drop an unfinished error scope without merging (branch writes stay
    /// isolated per the default no-write-back rule) and restore the
    /// main-path map.
    pub(super) fn discard_error_scope(&mut self) {
        if let Some(scope) = self.error_scope.take() {
            self.ctx.variables = scope.main_variables;
        }
    }

    /// Restore a checkpointed suspend: consume the typed `error_suspend`
    /// record from the execution state and rebuild the isolated scope plus
    /// the error namespace, so the resumed run continues from the branch
    /// target exactly as a fresh suspend entry would. Consuming the record
    /// keeps a resumed run from re-persisting it. No-op when nothing was
    /// suspended.
    pub async fn restore_suspended_error_branch(&mut self) {
        if self.error_scope.is_some() {
            return;
        }
        let entity = match self.entity.clone() {
            Some(entity) => entity,
            None => return,
        };
        let Some(state) = entity.state.write().await.take_error_suspend() else {
            return;
        };
        let event_bus = self.ctx.event_bus.clone();
        self.enter_error_branch(&entity, event_bus.as_deref(), state.summary, true)
            .await;
    }

    pub(super) async fn process_trigger_effects(&mut self, entity: &WorkflowExecutionEntity) {
        // Typed signal bus. Check signals that target this
        // execution and react accordingly.
        if let Some(signal_receiver) = &mut self.signal_receiver {
            let execution_id = self.ctx.execution_id.to_string();
            while let Some(signal) = signal_receiver.try_recv() {
                if *signal.target_execution_id() != execution_id {
                    continue;
                }
                match signal {
                    InternalSignal::StopWorkflow { .. } => {
                        let _ = entity.interruption().stop();
                        return;
                    }
                    InternalSignal::PauseWorkflow { .. } => {
                        let _ = entity.interruption().pause();
                    }
                    InternalSignal::ResumeWorkflow { .. } => {
                        let _ = entity.interruption().resume();
                    }
                    InternalSignal::SkipNode { node_id, .. } => {
                        // Record the node for skipping at dispatch time.
                        self.skipped_nodes.insert(node_id);
                    }
                    _ => {
                        // Result signals (SubworkflowResult, ScriptResult,
                        // AgentResult) are consumed by the agent loop,
                        // not the workflow coordinator.
                    }
                }
            }
        }
    }

    /// Route a terminal node failure through the error-branch table before
    /// interrupting the execution. Node routes win in declaration order, then
    /// the workflow catch-all default; no match keeps fail-fast. A matched
    /// continue route jumps to its target on an isolated overlay, a matched
    /// suspend route parks the execution for external recovery.
    pub(super) async fn route_node_failure(
        &mut self,
        entity: &WorkflowExecutionEntity,
        event_bus: Option<&EventBus>,
        failed_node_id: &str,
        node_type_str: &str,
        error: &crate::error::WorkflowError,
    ) -> WorkflowResult<ErrorFailureAction> {
        let category = crate::error_branch::classify_error(error);
        // External cancellation always wins: a pending Stop is never
        // re-routed into a branch.
        if matches!(
            entity.interruption().check(),
            Some(InterruptionSignal::Stop)
        ) {
            return Ok(ErrorFailureAction::Interrupt);
        }
        let Some(target) = self
            .traversal
            .error_table()
            .resolve(failed_node_id, category)
        else {
            return Ok(ErrorFailureAction::Interrupt);
        };
        if self.traversal.get_node(&target.target_node_id).is_none() {
            return Err(crate::error::WorkflowError::ConfigError {
                node_id: failed_node_id.to_string(),
                field: "error_route".to_string(),
                detail: format!(
                    "error route target '{}' does not exist in the graph",
                    target.target_node_id
                ),
            });
        }
        crate::error_branch::check_branch_budget(
            self.traversal.graph(),
            &target.target_node_id,
            self.ctx.options.max_steps,
            self.completed_nodes.len(),
        );
        if let Some(ref metrics) = self.ctx.metrics {
            metrics
                .node()
                .record_error(failed_node_id, node_type_str, category.as_str());
        }
        // `attempts` counts this node's recorded terminal failures (error
        // records), not engine-level retries: transport retries inside the
        // LLM/script layers are invisible to the coordinator.
        let attempts = entity
            .state
            .read()
            .await
            .error_records()
            .iter()
            .filter(|record| record.node_id.as_deref() == Some(failed_node_id))
            .count()
            .max(1) as u32;
        let summary = wf_types::workflow::error_branch::ErrorBranchSummary::new(
            error.to_string(),
            category,
            failed_node_id,
            attempts,
        );
        if target.suspend {
            return Ok(self
                .suspend_error_branch(entity, event_bus, summary, &target.target_node_id)
                .await);
        }
        // An explicit error branch takes precedence over a handler-requested
        // pause (the compression Fail fallback parks through the same pause
        // signal): the graph author handles this failure internally. A Stop
        // is never cleared — external cancellation always wins.
        if matches!(
            entity.interruption().check(),
            Some(InterruptionSignal::Pause)
        ) {
            let _ = entity.interruption().resume();
        }
        self.enter_error_branch(entity, event_bus, summary, false)
            .await;
        self.current_node_id = Some(target.target_node_id.clone());
        Ok(ErrorFailureAction::Continue)
    }
}
