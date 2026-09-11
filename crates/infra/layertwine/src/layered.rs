//! Layered State Machine Module
//!
//! Manages the layered pipeline: manual_edit / agent_edit → approval → integrated → staged.
//! Provides forward flow and reverse rollback with ironclad layer-gating rules.

pub mod agent;
pub mod approval;
pub mod integrated;
pub mod manual;
pub mod staged;
pub mod transition;

use crate::core::types::SnapshotId;
use crate::storage::repository::{AtomicOps, CheckpointPersist, PartitionStore};
use std::sync::Arc;

/// Merge result shared by all layer merge operations
///
/// Replaces FeatureMergeResult and UnifiedMergeResult with a single type.
#[derive(Debug, Clone)]
pub struct MergeResult {
    pub snapshot_id: SnapshotId,
    pub conflicts: Vec<crate::engine::merge::MergeConflict>,
}

impl MergeResult {
    /// Check if merge has conflicts
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }

    /// Get conflict count
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    /// Format all conflicts as Git-style markers
    pub fn format_conflicts(&self) -> String {
        if self.conflicts.is_empty() {
            return String::new();
        }
        let mut result = String::new();
        for (i, conflict) in self.conflicts.iter().enumerate() {
            result.push_str(&format!(
                "Conflict #{} (line {}):\n",
                i + 1,
                conflict.start_line
            ));
            result.push_str(&conflict.to_conflict_marker());
            result.push('\n');
        }
        result
    }
}

/// Hierarchical State Machine - Unified Operations Portal
///
/// Minimal handle over the storage backend kept for `wf-checkpoint`
/// compatibility. All workflow logic lives in the `layered::*` free
/// functions; branch switching and transactions are owned by
/// `wf-checkpoint`, not by this engine.
pub struct StateMachine<S> {
    storage: Arc<S>,
}

impl<S> StateMachine<S>
where
    S: PartitionStore + CheckpointPersist + AtomicOps,
{
    /// Creating a new state machine instance
    pub fn new(storage: Arc<S>) -> Self {
        StateMachine { storage }
    }

    /// Getting Storage Layer References
    pub fn storage(&self) -> &S {
        &self.storage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_storage_full;
    use std::sync::Arc;

    #[test]
    fn test_state_machine_new() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage);
        assert!(sm.storage().list_partitions().is_ok());
    }

    #[test]
    fn test_state_machine_storage_accessor() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage.clone());
        let retrieved = sm.storage();
        let partitions = retrieved.list_partitions();
        assert!(partitions.is_ok());
    }
}
