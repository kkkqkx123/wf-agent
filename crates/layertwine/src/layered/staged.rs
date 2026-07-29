//! staged layer operation
//!
//! The Staged layer is the last layer before commit submission.
//! It serves as the final preparation area for checkpoint commits.
//!
//! Responsibility:
//! 1. Accept merge results from unified layer (unique entry point)
//! 2. Support final validation before checkpoint submission
//! 3. Provide checkpoint commit functionality

use crate::checkpoint::types::{Checkpoint, CheckpointMetadata};
use crate::core::delta::Delta;
use crate::core::partition::Partition;
use crate::core::snapshot::Snapshot;
use crate::core::types::{CheckpointId, PartitionId, PartitionType, SnapshotId, SourceType};
use crate::engine::diff::diff_to_line_diff;
use crate::error::{LayertwineError, Result};
use crate::layered::MergeResult;
use crate::storage::repository::{
    CheckpointPersist, DeltaStore, FileNodeStore, PartitionStore, SnapshotStore,
};

/// Validation result for staged before commit
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    /// Staged is ready to commit
    Ready,
    /// Staged has unresolved conflicts
    HasConflicts(Vec<String>),
    /// Staged has other unresolved problems
    HasUnresolvedProblems(Vec<String>),
}

impl ValidationResult {
    /// Check if staged is ready to commit
    pub fn is_ready(&self) -> bool {
        matches!(self, ValidationResult::Ready)
    }

    /// Get error messages if validation failed
    pub fn get_errors(&self) -> Vec<String> {
        match self {
            ValidationResult::Ready => vec![],
            ValidationResult::HasConflicts(errors) => errors.clone(),
            ValidationResult::HasUnresolvedProblems(errors) => errors.clone(),
        }
    }
}

/// Fixed ID of the staged partition
pub fn staged_partition_id() -> PartitionId {
    uuid::Uuid::from_u128(0x6000_0000_0000_0000_0000_0000_0000_0000)
}

/// Getting or creating staged partitions
pub fn ensure_staged_partition<S: PartitionStore>(
    storage: &S,
    initial_snapshot_id: SnapshotId,
) -> Result<Partition> {
    let pid = staged_partition_id();
    match storage.get_partition(&pid) {
        Ok(p) => Ok(p),
        Err(_) => {
            let partition = Partition {
                id: pid,
                name: "staged".to_string(),
                current_snapshot: initial_snapshot_id,
                history: vec![initial_snapshot_id],
                partition_type: PartitionType::Staged,
            };
            storage
                .create_partition(&partition)
                .map_err(LayertwineError::Storage)?;
            Ok(partition)
        }
    }
}

/// Merge a single Integrated feature partition directly into Staged.
///
/// Uses three-way merge with the feature's own baseline:
///   baseline = feature.history[0]
///   ours     = staged.current_snapshot
///   theirs   = feature.current_snapshot
///
/// Replaces the former Unified intermediary layer. The three-way merge
/// ensures correctness when multiple features merge into staged sequentially.
pub fn merge_feature_to_staged<S>(storage: &S, feature_name: &str) -> Result<MergeResult>
where
    S: SnapshotStore + DeltaStore + FileNodeStore + PartitionStore,
{
    let staged_pid = staged_partition_id();
    let integrated_pid = crate::layered::integrated::integrated_partition_id(feature_name);

    let feature_part = storage.get_partition(&integrated_pid).map_err(|_| {
        LayertwineError::NotFound(format!("integrated partition '{}' not found", feature_name))
    })?;
    let baseline_id = feature_part.history.first().ok_or_else(|| {
        LayertwineError::StateMachine(format!(
            "integrated partition '{}' has empty history",
            feature_name
        ))
    })?;
    let baseline_snapshot = storage
        .get_snapshot(baseline_id)
        .map_err(LayertwineError::Storage)?;
    let feature_snapshot = storage
        .get_snapshot(&feature_part.current_snapshot)
        .map_err(LayertwineError::Storage)?;

    let staged_partition = storage.get_partition(&staged_pid).map_err(|_| {
        LayertwineError::NotFound("staged partition not found".into())
    })?;

    // If staged and feature already point to the same snapshot, no merge needed
    if staged_partition.current_snapshot == feature_part.current_snapshot {
        return Ok(MergeResult {
            snapshot_id: staged_partition.current_snapshot,
            conflicts: vec![],
        });
    }

    let staged_snapshot = storage
        .get_snapshot(&staged_partition.current_snapshot)
        .map_err(LayertwineError::Storage)?;

    // Reconstruct texts for three-way merge
    let baseline_text = crate::layered::transition::reconstruct_text(storage, &baseline_snapshot)?;
    let staged_text = crate::layered::transition::reconstruct_text(storage, &staged_snapshot)?;
    let feature_text = crate::layered::transition::reconstruct_text(storage, &feature_snapshot)?;

    // Three-way merge: baseline (base), staged (ours), feature (theirs)
    let (merged_text, conflicts) =
        crate::engine::merge::merge_texts(&baseline_text, &staged_text, &feature_text);

    let has_conflicts = !conflicts.is_empty();

    let merge_diff = diff_to_line_diff(&staged_text, &merged_text);
    if merge_diff.is_empty() {
        return Ok(MergeResult {
            snapshot_id: staged_partition.current_snapshot,
            conflicts,
        });
    }

    let feature_deltas = storage
        .get_deltas(&feature_snapshot.deltas)
        .map_err(LayertwineError::Storage)?;
    let merge_file = feature_deltas
        .last()
        .map(|d| d.file.clone())
        .unwrap_or_else(|| staged_snapshot.file.clone());
    let merge_delta = Delta::new(merge_file, merge_diff, SourceType::Manual);
    storage
        .store_delta(&merge_delta)
        .map_err(LayertwineError::Storage)?;

    let new_snapshot = Snapshot::merge(
        vec![&staged_snapshot, &feature_snapshot],
        merge_delta.id,
        PartitionType::Staged.name(),
        has_conflicts,
    );
    storage
        .store_snapshot(&new_snapshot, b"")
        .map_err(LayertwineError::Storage)?;
    storage
        .update_pointer(&staged_pid, &new_snapshot.id)
        .map_err(LayertwineError::Storage)?;

    Ok(MergeResult {
        snapshot_id: new_snapshot.id,
        conflicts,
    })
}

/// Merge multiple Integrated feature partitions directly into Staged.
///
/// Each feature is merged sequentially via three-way merge using its own baseline.
/// Features are merged one at a time, each accumulating into staged.
/// This replaces the former `merge_features_to_unified` + `merge_unified_to_staged` pattern.
pub fn merge_features_to_staged<S>(storage: &S, feature_names: &[String]) -> Result<MergeResult>
where
    S: SnapshotStore + DeltaStore + FileNodeStore + PartitionStore,
{
    if feature_names.is_empty() {
        return Err(LayertwineError::General(
            "at least one feature required".into(),
        ));
    }

    let mut all_conflicts = Vec::new();
    let mut last_snapshot_id = None;

    for name in feature_names {
        let result = merge_feature_to_staged(storage, name)?;
        all_conflicts.extend(result.conflicts);
        last_snapshot_id = Some(result.snapshot_id);
    }

    Ok(MergeResult {
        snapshot_id: last_snapshot_id.unwrap(),
        conflicts: all_conflicts,
    })
}

/// Validate staged before commit
///
/// Checks if staged is ready to commit by:
/// 1. Checking for unresolved conflicts in the staged snapshot
/// 2. Checking for other problems
pub fn validate_staged_for_commit<S>(storage: &S) -> Result<ValidationResult>
where
    S: SnapshotStore + PartitionStore + DeltaStore + FileNodeStore,
{
    let staged_pid = staged_partition_id();
    let staged = storage
        .get_partition(&staged_pid)
        .map_err(|_| LayertwineError::NotFound("staged partition not found".into()))?;

    let staged_snapshot = storage
        .get_snapshot(&staged.current_snapshot)
        .map_err(LayertwineError::Storage)?;

    let mut problems = Vec::new();

    if staged_snapshot.has_conflicts {
        problems.push(
            "Staged snapshot has unresolved merge conflicts. Resolve conflicts before committing."
                .to_string(),
        );
    }

    if problems.is_empty() {
        Ok(ValidationResult::Ready)
    } else {
        Ok(ValidationResult::HasUnresolvedProblems(problems))
    }
}

/// Submit staged as Checkpoint
///
/// 1. Get staged partition current snapshot
/// 2. Get current branch head from CheckpointPersist
/// 3. Build a Checkpoint with the snapshot as baseline
/// 4. Store the checkpoint and update branch head via CheckpointPersist
/// 5. Return the new CheckpointId
///
/// Note: DAG is built dynamically from Checkpoint relationships and is not persisted.
pub fn commit_staged_to_checkpoint<S>(
    storage: &S,
    branch_name: &str,
    message: &str,
    author: &str,
) -> Result<CheckpointId>
where
    S: SnapshotStore + PartitionStore + CheckpointPersist,
{
    // 1. Get staged partition
    let staged_pid = staged_partition_id();
    let staged_partition = storage
        .get_partition(&staged_pid)
        .map_err(|_| LayertwineError::NotFound("staged partition not found".into()))?;
    let current_snapshot_id = staged_partition.current_snapshot;

    // 2. Get or create branch
    let branch_head = match storage.get_branch(branch_name) {
        Ok(b) => b.head,
        Err(_) => {
            // First commit: create initial branch pointing to the staged snapshot
            let branch = crate::checkpoint::branch::Branch::new(branch_name, current_snapshot_id);
            storage
                .store_branch(&branch)
                .map_err(LayertwineError::Storage)?;
            current_snapshot_id
        }
    };

    // 3. Build Checkpoint
    let metadata = CheckpointMetadata::new(author, message);
    let cp = Checkpoint::new(vec![current_snapshot_id], vec![branch_head], metadata);
    let cp_id = cp.id;

    // 4. Store checkpoint
    storage
        .store_checkpoint(&cp)
        .map_err(LayertwineError::Storage)?;

    // 5. Update branch head
    storage
        .update_branch_head(branch_name, &cp_id)
        .map_err(LayertwineError::Storage)?;

    Ok(cp_id)
}

/// Empty staged partition (reset to initial state)
pub fn reset_staged<S: PartitionStore>(storage: &S, base_snapshot_id: SnapshotId) -> Result<()> {
    let pid = staged_partition_id();
    storage
        .update_pointer(&pid, &base_snapshot_id)
        .map_err(LayertwineError::Storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::file_node::FileNode;
    use crate::core::types::{PartitionType, SourceType};
    use crate::engine::diff::diff_to_line_diff;
    use crate::storage::repository::FileNodeStore;
    use crate::storage::SqliteStorage;
    use crate::test_utils::{create_initial_snapshot, setup_storage_full};

    fn create_snapshot_with_content(
        storage: &SqliteStorage,
        parent_id: &crate::core::types::SnapshotId,
        content: &str,
        partition_type: &str,
    ) -> crate::core::types::SnapshotId {
        let parent = storage.get_snapshot(parent_id).unwrap();
        let file_node = FileNode::new(std::path::PathBuf::from("test.txt"), content.as_bytes());
        storage
            .store_file_node(&file_node, content.as_bytes())
            .unwrap();

        let parent_text = crate::layered::transition::reconstruct_text(storage, &parent).unwrap();
        let diff = diff_to_line_diff(&parent_text, content);
        let delta = Delta::new(file_node, diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();

        let snap = Snapshot::from_parent(&parent, delta.id, partition_type.to_string());
        storage.store_snapshot(&snap, b"").unwrap();
        snap.id
    }

    #[test]
    fn test_ensure_staged_partition() {
        let storage = setup_storage_full();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);

        let p1 = ensure_staged_partition(&storage, initial_id).unwrap();
        let p2 = ensure_staged_partition(&storage, initial_id).unwrap();
        assert_eq!(p1.id, p2.id);
    }

    #[test]
    fn test_merge_feature_to_staged() {
        let storage = setup_storage_full();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        ensure_staged_partition(&storage, initial_id).unwrap();

        // Create an integrated (feature) partition with modified content
        let feature_name = "test-feature";
        let integrated_pid = crate::layered::integrated::integrated_partition_id(feature_name);
        let feature_snap_id = create_snapshot_with_content(
            &storage,
            &initial_id,
            "base\nfeature-added\n",
            "integrated/test-feature",
        );
        let integrated_part = Partition {
            id: integrated_pid,
            name: format!("integrated/{}", feature_name),
            current_snapshot: feature_snap_id,
            history: vec![initial_id, feature_snap_id],
            partition_type: PartitionType::Integrated(feature_name.to_string()),
        };
        storage.create_partition(&integrated_part).unwrap();

        let merged_id = merge_feature_to_staged(&storage, feature_name).unwrap();
        assert!(
            merged_id.snapshot_id != initial_id,
            "should create new snapshot when there are changes"
        );

        let staged = storage.get_partition(&staged_partition_id()).unwrap();
        assert_eq!(staged.current_snapshot, merged_id.snapshot_id);

        let merged_snap = storage.get_snapshot(&merged_id.snapshot_id).unwrap();
        assert_eq!(merged_snap.parents.len(), 2);
    }

    #[test]
    fn test_merge_feature_to_staged_no_changes() {
        let storage = setup_storage_full();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        ensure_staged_partition(&storage, initial_id).unwrap();

        // Create an integrated partition without modifications
        let feature_name = "test-feature";
        let integrated_pid = crate::layered::integrated::integrated_partition_id(feature_name);
        let integrated_part = Partition {
            id: integrated_pid,
            name: format!("integrated/{}", feature_name),
            current_snapshot: initial_id,
            history: vec![initial_id],
            partition_type: PartitionType::Integrated(feature_name.to_string()),
        };
        storage.create_partition(&integrated_part).unwrap();

        let result = merge_feature_to_staged(&storage, feature_name).unwrap();
        assert_eq!(
            result.snapshot_id, initial_id,
            "should return initial id when no changes"
        );
    }

    #[test]
    fn test_commit_staged_to_checkpoint() {
        let storage = setup_storage_full();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        ensure_staged_partition(&storage, initial_id).unwrap();

        let cp_id =
            commit_staged_to_checkpoint(&storage, "main", "test commit", "test-author").unwrap();

        let checkpoint = storage.get_checkpoint(&cp_id).unwrap();
        assert_eq!(checkpoint.baseline_snapshots.len(), 1);
        assert_eq!(checkpoint.baseline_snapshots[0], initial_id);

        let branch = storage.get_branch("main").unwrap();
        assert_eq!(branch.head, cp_id);
    }

    #[test]
    fn test_commit_staged_to_checkpoint_multiple() {
        let storage = setup_storage_full();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        ensure_staged_partition(&storage, initial_id).unwrap();

        let cp_id1 =
            commit_staged_to_checkpoint(&storage, "main", "first commit", "test-author").unwrap();
        let cp_id2 =
            commit_staged_to_checkpoint(&storage, "main", "second commit", "test-author").unwrap();

        assert_ne!(
            cp_id1, cp_id2,
            "different commits should have different IDs"
        );

        let branch = storage.get_branch("main").unwrap();
        assert_eq!(
            branch.head, cp_id2,
            "branch head should point to latest commit"
        );
    }

    #[test]
    fn test_reset_staged() {
        let storage = setup_storage_full();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        ensure_staged_partition(&storage, initial_id).unwrap();

        let file_node = FileNode::new(std::path::PathBuf::from("test.txt"), b"base\nmodified\n");
        storage
            .store_file_node(&file_node, b"base\nmodified\n")
            .unwrap();
        let diff = diff_to_line_diff("base\n", "base\nmodified\n");
        let delta = Delta::new(file_node, diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();
        let snap = storage.get_snapshot(&initial_id).unwrap();
        let new_snap = Snapshot::from_parent(&snap, delta.id, PartitionType::Staged.name());
        storage.store_snapshot(&new_snap, b"").unwrap();
        let staged_pid = staged_partition_id();
        storage.update_pointer(&staged_pid, &new_snap.id).unwrap();

        let staged = storage.get_partition(&staged_pid).unwrap();
        assert_ne!(staged.current_snapshot, initial_id);

        reset_staged(&storage, initial_id).unwrap();
        let staged = storage.get_partition(&staged_pid).unwrap();
        assert_eq!(staged.current_snapshot, initial_id);
    }

    #[test]
    fn test_reset_staged_at_base() {
        let storage = setup_storage_full();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        ensure_staged_partition(&storage, initial_id).unwrap();

        reset_staged(&storage, initial_id).unwrap();
        let staged = storage.get_partition(&staged_partition_id()).unwrap();
        assert_eq!(staged.current_snapshot, initial_id);
    }
}
