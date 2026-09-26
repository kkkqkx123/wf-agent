use wf_core::EventBus;
use wf_execution_shared::types::state_manager::StateManager;

use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;
use crate::state::WorkflowExecutionStateSnapshot;

use super::WorkflowCoordinator;

impl WorkflowCoordinator {
    /// Loop-aware navigation backstop. Tracks two separate counters:
    ///
    /// 1. **`navigation_count`**: Counts node navigations *outside* any loop
    ///    body. Cycles in DAG edges that never pass through a LOOP_START node
    ///    keep accumulating and eventually trip the detector.
    ///
    /// 2. **`loop_navigation_count`**: Counts node navigations *inside* the
    ///    current loop body. Reset each time a LOOP_START is re-entered.
    ///    This catches loops whose body alone is too large relative to the
    ///    graph size (e.g., a loop body with 100 nodes and a multiplier of 5
    ///    would trigger at 500 iterations within a single pass).
    ///
    /// Both counters use `total_node_count * max_navigation_multiplier` as
    /// the budget.
    pub(super) fn check_navigation_backstop(&mut self, node_id: &str) -> WorkflowResult<()> {
        let node_type = self
            .traversal
            .get_node(node_id)
            .map(|n| n.node_type.as_str());

        match node_type {
            Some("LOOP_START") => {
                // Entering a loop body: reset the loop-local counter and
                // mark that we are inside a loop scope. The global
                // navigation_count is *not* reset here – it continues to
                // track non-loop cycles.
                self.in_loop_body = true;
                self.loop_navigation_count = 0;
            }
            Some("LOOP_END") => {
                // Exiting the loop body: clear the in-loop flag. The next
                // node (outside the loop) will resume incrementing
                // navigation_count.
                self.in_loop_body = false;
                self.loop_navigation_count = 0;
            }
            _ => {}
        }

        if self.in_loop_body {
            self.loop_navigation_count += 1;
            let max_allowed = self.total_node_count * self.max_navigation_multiplier;
            if self.loop_navigation_count > max_allowed && max_allowed > 0 {
                return Err(crate::error::WorkflowError::CoordinatorError(format!(
                    "Loop body exceeded navigation limit: {} node visits (max {} = {} nodes x {})",
                    self.loop_navigation_count,
                    max_allowed,
                    self.total_node_count,
                    self.max_navigation_multiplier
                )));
            }
        } else {
            self.navigation_count += 1;
            let max_allowed = self.total_node_count * self.max_navigation_multiplier;
            if self.navigation_count > max_allowed && max_allowed > 0 {
                return Err(crate::error::WorkflowError::CoordinatorError(format!(
                    "Infinite loop detected: {} navigations exceeded max {} ({} nodes x {})",
                    self.navigation_count,
                    max_allowed,
                    self.total_node_count,
                    self.max_navigation_multiplier
                )));
            }
        }
        Ok(())
    }

    /// Completed-node skip decision. Loop iterations legitimately re-visit
    /// completed nodes: a completed node re-executes when a loop is active
    /// and the node belongs to an earlier iteration (missing from the current
    /// iteration's completion list) or is a loop control node
    /// (LOOP_START/LOOP_END, always idempotent). Completed-node skipping
    /// otherwise applies (checkpoint resume semantics). Returns `true` when
    /// the caller should `continue` the main loop.
    pub(super) async fn skip_completed_node(
        &mut self,
        entity: &WorkflowExecutionEntity,
        event_bus: Option<&EventBus>,
        node_id: &str,
    ) -> WorkflowResult<bool> {
        let top_loop = crate::loop_state::stack(&self.ctx.variables)
            .pop()
            .filter(|s| !s.loop_id.is_empty());
        let is_loop_control = self
            .traversal
            .get_node(node_id)
            .is_some_and(|n| matches!(n.node_type.as_str(), "LOOP_START" | "LOOP_END"));
        let reexec_in_loop = top_loop
            .as_ref()
            .is_some_and(|s| is_loop_control || !s.iteration_nodes.iter().any(|n| n == node_id));
        if self.completed_nodes.iter().any(|n| n == node_id) && !reexec_in_loop {
            self.emit_event(
                event_bus,
                wf_types::events::EventType::NodeSkipped,
                entity,
                &serde_json::json!({
                    "node_id": node_id,
                    "reason": "already_completed",
                }),
            )
            .await;
            self.current_node_id = self.determine_next_node_without_output().await?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Snapshot of the owned entity's execution state.
    pub async fn state_snapshot(&self) -> WorkflowResult<WorkflowExecutionStateSnapshot> {
        let entity = self.entity.as_ref().ok_or_else(|| {
            crate::error::WorkflowError::CoordinatorError(
                "Entity not set on WorkflowCoordinator".to_string(),
            )
        })?;
        Ok(entity.state.read().await.create_snapshot().await?)
    }
}
