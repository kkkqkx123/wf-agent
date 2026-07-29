//! Layered State Machine Module (Phase 3)
//!
//! Manages the six-layer pipeline: manual_edit → agent_edit → approval → integrated → unified → staged.
//! Provides forward flow and reverse rollback with ironclad layer-gating rules.

pub mod agent;
pub mod api;
pub mod approval;
pub mod integrated;
pub mod manual;
pub mod staged;
pub mod transition;

use crate::core::layer::Layer;
use crate::core::partition::Partition;
use crate::core::types::{CheckpointId, PartitionId, PartitionType, SnapshotId};
use crate::error::{LayertwineError, Result};
use crate::storage::repository::{AtomicOps, CheckpointPersist, LayerStore, PartitionStore};
use std::collections::HashMap;
use std::sync::Arc;

/// Unified merge result for all merge operations
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
/// Holds storage tier references and provides partition access and state flow interfaces for each tier.
pub struct StateMachine<S> {
    storage: Arc<S>,
}

impl<S> StateMachine<S>
where
    S: PartitionStore + CheckpointPersist + LayerStore + AtomicOps,
{
    /// Creating a new state machine instance
    pub fn new(storage: Arc<S>) -> Self {
        StateMachine { storage }
    }

    /// Getting Storage Layer References
    pub fn storage(&self) -> &S {
        &self.storage
    }

    // Partition access methods -

    /// Get the specified partition (read-only)
    pub fn get_partition(&self, partition_id: &PartitionId) -> Result<Partition> {
        self.storage
            .get_partition(partition_id)
            .map_err(LayertwineError::Storage)
    }

    /// Getting or creating partitions
    pub fn get_or_create_partition(
        &self,
        partition_id: &PartitionId,
        partition: &Partition,
    ) -> Result<Partition> {
        // First try to get
        match self.storage.get_partition(partition_id) {
            Ok(p) => Ok(p),
            Err(_) => {
                self.storage
                    .create_partition(partition)
                    .map_err(LayertwineError::Storage)?;
                Ok(partition.clone())
            }
        }
    }

    /// Updating the partition pointer
    pub fn update_partition_pointer(
        &self,
        partition_id: &PartitionId,
        snapshot_id: &SnapshotId,
    ) -> Result<()> {
        self.storage
            .update_pointer(partition_id, snapshot_id)
            .map_err(LayertwineError::Storage)
    }

    // Layer management -

    /// Creating a Default Layer
    pub fn create_layer(&self, layer_type: &crate::core::types::LayerType) -> Layer {
        Layer::new(layer_type.clone())
    }

    /// Switch branches and synchronize the layer status
    ///
    /// 1. Get the head checkpoint for the target branch
    /// 2. Reset the staged partition to the first snapshot in the checkpoint
    /// 3. Clear other layer states (approval, agent_edit) if needed
    pub fn switch_branch(&self, branch_name: &str) -> Result<CheckpointId> {
        let branch = self
            .storage
            .get_branch(branch_name)
            .map_err(LayertwineError::Storage)?;
        let head_cp = self
            .storage
            .get_checkpoint(&branch.head)
            .map_err(LayertwineError::Storage)?;
        if head_cp.baseline_snapshots.is_empty() {
            return Err(LayertwineError::Checkpoint(
                "branch head checkpoint has no snapshots".into(),
            ));
        }
        let base_snapshot = head_cp.baseline_snapshots[0];

        // Reset staged partition to the branch's base snapshot
        let staged_pid = crate::layered::staged::staged_partition_id();
        match self.storage.get_partition(&staged_pid) {
            Ok(_) => {
                self.storage
                    .update_pointer(&staged_pid, &base_snapshot)
                    .map_err(LayertwineError::Storage)?;
            }
            Err(_) => {
                let partition =
                    Partition::new("staged".to_string(), PartitionType::Staged, base_snapshot);
                self.storage
                    .create_partition(&partition)
                    .map_err(LayertwineError::Storage)?;
            }
        }

        Ok(branch.head)
    }

    /// Sync the `layers` table to reflect current partition state.
    ///
    /// Reads all partitions, groups them by layer type, and writes
    /// corresponding entries into the `layers` table.
    pub fn sync_layers(&self) -> Result<()> {
        let partitions = self
            .storage
            .list_partitions()
            .map_err(LayertwineError::Storage)?;

        let mut layer_map: HashMap<crate::core::types::LayerType, Vec<PartitionId>> =
            HashMap::new();
        for p in &partitions {
            let lt = p.partition_type.to_layer();
            layer_map.entry(lt).or_default().push(p.id);
        }

        for (lt, pids) in &layer_map {
            let mut layer = Layer::new(lt.clone());
            layer.partitions = pids.clone();
            self.storage
                .store_layer(&layer)
                .map_err(LayertwineError::Storage)?;
        }

        Ok(())
    }

    // Transaction support -

    /// Execute operations with SAVEPOINT-based atomicity.
    ///
    /// Delegates to the storage backend's `AtomicOps::with_atomic`, which on SQLite
    /// provides proper SAVEPOINT-based transaction guarantees. Uses `parking_lot::ReentrantMutex`
    /// internally, so reentrant locking (calling other storage methods from within the closure)
    /// is safe and does NOT deadlock.
    pub fn with_transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&S) -> Result<T>,
    {
        let result = std::cell::RefCell::new(None::<Result<T>>);

        self.storage
            .with_atomic(|s| {
                result.borrow_mut().replace(f(s));
                Ok(())
            })
            .map_err(LayertwineError::Storage)?;

        result.into_inner().unwrap_or_else(|| {
            Err(LayertwineError::Transaction(
                "transaction did not produce a result".into(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::delta::Delta;
    use crate::core::file_node::FileNode;
    use crate::core::snapshot::Snapshot;
    use crate::core::types::{LayerType, SourceType};
    use crate::storage::repository::{DeltaStore, FileNodeStore, PartitionStore, SnapshotStore};
    use crate::test_utils::{create_initial_snapshot, setup_storage_full};
    use std::sync::Arc;

    #[test]
    fn test_state_machine_new() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage);
        let result = sm.get_partition(&uuid::Uuid::now_v7());
        assert!(
            result.is_err(),
            "non-existent partition should return error"
        );
    }

    #[test]
    fn test_state_machine_create_layer() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage);

        let manual_layer = sm.create_layer(&LayerType::ManualEdit);
        assert_eq!(manual_layer.layer_type, LayerType::ManualEdit);
        assert!(manual_layer.partitions.is_empty());

        let agent_layer = sm.create_layer(&LayerType::AgentEdit);
        assert_eq!(agent_layer.layer_type, LayerType::AgentEdit);

        let approval_layer = sm.create_layer(&LayerType::Approval);
        assert_eq!(approval_layer.layer_type, LayerType::Approval);

        let staged_layer = sm.create_layer(&LayerType::Staged);
        assert_eq!(staged_layer.layer_type, LayerType::Staged);
    }

    #[test]
    fn test_state_machine_update_partition_pointer() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage.clone());

        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        let pid = uuid::Uuid::now_v7();
        let partition = Partition {
            id: pid,
            name: "test".to_string(),
            current_snapshot: initial_id,
            history: vec![initial_id],
            partition_type: PartitionType::Manual,
        };
        storage.create_partition(&partition).unwrap();

        let file_node = FileNode::new(std::path::PathBuf::from("test.txt"), b"base\nmodified\n");
        storage
            .store_file_node(&file_node, b"base\nmodified\n")
            .unwrap();
        let diff = crate::engine::diff::diff_to_line_diff("base\n", "base\nmodified\n");
        let delta = Delta::new(file_node, diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();
        let snap = storage.get_snapshot(&initial_id).unwrap();
        let new_snap = Snapshot::from_parent(&snap, delta.id, "manual".to_string());
        storage.store_snapshot(&new_snap, b"").unwrap();

        let result = sm.update_partition_pointer(&pid, &new_snap.id);
        assert!(result.is_ok());

        let updated = storage.get_partition(&pid).unwrap();
        assert_eq!(updated.current_snapshot, new_snap.id);
        assert_eq!(
            updated.history.len(),
            2,
            "history should contain both snapshots"
        );
        assert_eq!(updated.history[0], initial_id);
        assert_eq!(updated.history[1], new_snap.id);
    }

    #[test]
    fn test_state_machine_storage_accessor() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage.clone());
        let retrieved = sm.storage();
        let partitions = retrieved.list_partitions();
        assert!(partitions.is_ok());
    }

    #[test]
    fn test_state_machine_get_partition() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage.clone());

        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        let pid = uuid::Uuid::now_v7();
        let partition = Partition {
            id: pid,
            name: "test".to_string(),
            current_snapshot: initial_id,
            history: vec![initial_id],
            partition_type: PartitionType::Manual,
        };
        storage.create_partition(&partition).unwrap();

        let result = sm.get_partition(&pid);
        assert!(result.is_ok());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.id, pid);
        assert_eq!(retrieved.name, "test");
    }

    #[test]
    fn test_state_machine_get_or_create_partition() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage.clone());

        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        let pid = uuid::Uuid::now_v7();
        let partition = Partition {
            id: pid,
            name: "test".to_string(),
            current_snapshot: initial_id,
            history: vec![initial_id],
            partition_type: PartitionType::Manual,
        };

        let result = sm.get_or_create_partition(&pid, &partition);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, pid);

        let result2 = sm.get_or_create_partition(&pid, &partition);
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap().id, pid);
    }

    #[test]
    fn test_state_machine_switch_branch_nonexistent() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage);

        let result = sm.switch_branch("nonexistent-branch");
        assert!(
            result.is_err(),
            "switching to nonexistent branch should error"
        );
    }

    #[test]
    fn test_state_machine_sync_layers() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage.clone());

        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);

        let storage_ref = &*storage;
        crate::layered::manual::ensure_manual_partition(storage_ref, initial_id).unwrap();
        crate::layered::staged::ensure_staged_partition(storage_ref, initial_id).unwrap();

        let result = sm.sync_layers();
        assert!(result.is_ok());

        let manual_layer = sm.storage().get_layer(&LayerType::ManualEdit).unwrap();
        assert_eq!(manual_layer.partitions.len(), 1);

        let staged_layer = sm.storage().get_layer(&LayerType::Staged).unwrap();
        assert_eq!(staged_layer.partitions.len(), 1);
    }

    #[test]
    fn test_state_machine_with_transaction() {
        let storage = Arc::new(setup_storage_full());
        let sm = StateMachine::new(storage.clone());

        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);

        let result = sm.with_transaction(|storage| {
            let pid = uuid::Uuid::now_v7();
            let partition = Partition {
                id: pid,
                name: "transaction-test".to_string(),
                current_snapshot: initial_id,
                history: vec![initial_id],
                partition_type: PartitionType::Manual,
            };
            storage.create_partition(&partition)?;
            Ok(pid)
        });

        assert!(result.is_ok());
        let pid = result.unwrap();
        let retrieved = storage.get_partition(&pid);
        assert!(retrieved.is_ok());
    }
}
