use std::sync::Arc;

use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_types::Id;

use crate::registry::AgentLoopRegistry;

/// Reclaim guard for one dispatched child execution.
///
/// The guard is armed before the child run starts and disarmed by `finish`
/// once the run settles. A settled run also unlinks the child from the
/// parent hierarchy, so the parent's child list tracks live children only.
///
/// When the driving future is dropped before settling — a tool-level
/// timeout unwinds the synchronous dispatch, or the background task is
/// aborted — the child would otherwise stay registered in a non-terminal
/// state forever, holding its capacity-gate permit and its parent link.
/// The drop path reclaims the registry slot and the permit in that case.
/// An already-settled entity keeps its terminal record: only the parent
/// link is removed.
pub(super) struct ChildRunReclaim {
    registry: Arc<AgentLoopRegistry>,
    child_id: Id,
    parent_manager: Option<Arc<wf_core::hierarchy::ExecutionHierarchyManager>>,
    armed: bool,
}

impl ChildRunReclaim {
    pub(super) fn arm(
        registry: Arc<AgentLoopRegistry>,
        child_id: Id,
        parent_manager: Option<Arc<wf_core::hierarchy::ExecutionHierarchyManager>>,
    ) -> Self {
        Self {
            registry,
            child_id,
            parent_manager,
            armed: true,
        }
    }

    /// The run settled: unlink the child from the parent hierarchy and
    /// disarm the guard.
    pub(super) fn finish(mut self) {
        self.unlink_parent();
        self.armed = false;
    }

    fn unlink_parent(&self) {
        if let Some(manager) = &self.parent_manager {
            manager.remove_child(
                self.child_id.as_str(),
                &wf_types::execution::ExecutionType::Agent,
            );
        }
    }
}

impl Drop for ChildRunReclaim {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // An absent entity never registered; a terminal entity settled
        // through its own state transition. Only a non-terminal entity is a
        // phantom left behind by the dropped driving future.
        let settled = self
            .registry
            .get(&self.child_id)
            .map(|entity| entity.status().is_terminal())
            .unwrap_or(true);
        if !settled {
            if let Some(entity) = self.registry.get(&self.child_id) {
                entity.release_gate_permit();
            }
            self.registry.unregister(&self.child_id);
            tracing::warn!(
                child_execution_id = %self.child_id,
                "dispatched child execution dropped before settling; reclaimed registry slot and capacity permit"
            );
        }
        self.unlink_parent();
    }
}
