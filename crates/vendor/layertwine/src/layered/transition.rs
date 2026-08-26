//! Inter-layer flow logic
//!
//! Define all allowed forward/reverse flow operations, and state machine irony checks.

use crate::backup::backup_repo::BackupRepo;
use crate::core::snapshot::Snapshot;
use crate::core::types::{BackupId, LayerType, PartitionId, SnapshotId};
use crate::engine::merge::apply_deltas;
use crate::error::{LayertwineError, Result};
use crate::storage::repository::{DeltaStore, FileNodeStore, PartitionStore, SnapshotStore};

// ===== Allowable Direction of Flow =====

/// Positive flow type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardTransition {
    /// manual_edit → staged
    ManualToStaged,
    /// agent_edit → approval (Agent Raw → Agent Approval)
    AgentToApproval,
    /// approval → integrated
    ApprovalToIntegrated,
    /// integrated → staged (replaces former integrated → unified + unified → staged)
    IntegratedToStaged,
}

/// Type of reverse flow
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackTransition {
    /// staged → manual_edit
    StagedToManual,
    /// staged → approval
    StagedToApproval,
    /// staged → agent_edit
    StagedToAgentRaw,
    /// approval → agent_edit
    ApprovalToAgentRaw,
}

// ===== Iron Law Check =====

/// Check if positive flow is allowed
///
/// Iron rule 1: No cross-layer flows - all flows must pass through neighboring layers
pub fn check_forward_valid(from: &LayerType, to: &LayerType) -> Result<()> {
    let valid = matches!(
        (from, to),
        (LayerType::ManualEdit, LayerType::Staged)
            | (LayerType::AgentEdit, LayerType::Approval)
            | (LayerType::Approval, LayerType::Integrated)
            | (LayerType::Integrated, LayerType::Staged)
    );

    if !valid {
        return Err(LayertwineError::StateMachine(format!(
            "Ironclad check failed: impermissible cross-layer flow {:?} → {:?}",
            from, to
        )));
    }
    Ok(())
}

/// Check if reverse fallback is allowed
///
/// Ironclad Rule #2: No Reverse Writes - Fallback Only Toggles Pointer
pub fn check_rollback_valid(from: &LayerType, to: &LayerType) -> Result<()> {
    let valid = matches!(
        (from, to),
        (LayerType::Staged, LayerType::ManualEdit)
            | (LayerType::Staged, LayerType::AgentEdit)
            | (LayerType::Staged, LayerType::Approval)
            | (LayerType::Approval, LayerType::AgentEdit)
    );

    if !valid {
        return Err(LayertwineError::StateMachine(format!(
            "Ironclad check failed: impermissible cross-level fallback {:?} → {:?}",
            from, to
        )));
    }
    Ok(())
}

// ===== Pointer operations =====

/// Copy current_snapshot pointer from one partition to another (pointer-only, no data writes)
pub fn migrate_between_partitions<S: PartitionStore>(
    storage: &S,
    from_partition_id: &PartitionId,
    to_partition_id: &PartitionId,
) -> Result<()> {
    let from_partition = storage
        .get_partition(from_partition_id)
        .map_err(|_| LayertwineError::NotFound("source partition not found".into()))?;

    storage
        .update_pointer(to_partition_id, &from_partition.current_snapshot)
        .map_err(LayertwineError::Storage)?;

    Ok(())
}

// ===== Forward transitions =====

/// Implementation of positive flow
///
/// Automatically schedules operation functions to each layer based on the ForwardTransition type.
pub fn execute_forward<S>(
    storage: &S,
    transition: ForwardTransition,
    params: &[&str], // Optional parameters: agent_id, integrated_name, etc.
) -> Result<SnapshotId>
where
    S: SnapshotStore + DeltaStore + FileNodeStore + PartitionStore,
{
    match transition {
        ForwardTransition::ManualToStaged => {
            check_forward_valid(&LayerType::ManualEdit, &LayerType::Staged)?;
            crate::layered::manual::merge_manual_to_staged(storage, None)
        }
        ForwardTransition::AgentToApproval => {
            check_forward_valid(&LayerType::AgentEdit, &LayerType::Approval)?;
            let agent_id = params.first().ok_or_else(|| {
                LayertwineError::StateMachine("AgentToApproval requires agent_id parameter".into())
            })?;
            crate::layered::agent::move_agent_to_approval(
                storage,
                &crate::core::types::AgentInstanceId(agent_id.to_string()),
            )
        }
        ForwardTransition::ApprovalToIntegrated => {
            let agent_id = params.first().ok_or_else(|| {
                LayertwineError::StateMachine(
                    "ApprovalToIntegrated requires agent_id parameter".into(),
                )
            })?;
            let integrated_name = params.get(1).ok_or_else(|| {
                LayertwineError::StateMachine(
                    "ApprovalToIntegrated requires integrated_name parameter".into(),
                )
            })?;
            crate::layered::integrated::merge_agent_to_feature(
                storage,
                &crate::core::types::AgentInstanceId(agent_id.to_string()),
                integrated_name,
            )
            .map(|r| r.snapshot_id)
        }
        ForwardTransition::IntegratedToStaged => {
            // integrated_names passed via params, separated by commas
            let names_str = params.first().ok_or_else(|| {
                LayertwineError::StateMachine(
                    "IntegratedToStaged requires integrated_names parameter".into(),
                )
            })?;
            let names: Vec<String> = names_str.split(',').map(|s| s.trim().to_string()).collect();
            crate::layered::staged::merge_features_to_staged(storage, &names, None)
                .map(|r| r.snapshot_id)
        }
    }
}

// ===== Rollback operations =====

/// Partition itself back: current = history.pop()
///
/// Only switches pointers and does not modify any immutable data (Iron Law 2).
pub fn rollback_partition<S: PartitionStore>(
    storage: &S,
    partition_id: &PartitionId,
) -> Result<SnapshotId> {
    let partition = storage
        .get_partition(partition_id)
        .map_err(|_| LayertwineError::NotFound("partition not found".into()))?;

    if partition.history.len() <= 1 {
        return Err(LayertwineError::StateMachine(
            "cannot rollback: only one snapshot in history".into(),
        ));
    }

    let prev_id = partition.history[partition.history.len() - 2];
    storage
        .update_pointer(partition_id, &prev_id)
        .map_err(LayertwineError::Storage)?;

    Ok(prev_id)
}

/// Fallback staged to the specified layer
///
/// Finds the target layer source from the parents of the staged current snapshot.
pub fn rollback_staged_to_layer<S>(storage: &S, target_layer: LayerType) -> Result<SnapshotId>
where
    S: SnapshotStore + PartitionStore,
{
    let staged_pid = crate::layered::staged::staged_partition_id();
    let staged_partition = storage
        .get_partition(&staged_pid)
        .map_err(|_| LayertwineError::NotFound("staged partition not found".into()))?;

    let staged_snapshot = storage
        .get_snapshot(&staged_partition.current_snapshot)
        .map_err(LayertwineError::Storage)?;

    // Find the source of the target layer from staged parents
    for parent_id in &staged_snapshot.parents {
        let parent_snapshot = storage
            .get_snapshot(parent_id)
            .map_err(LayertwineError::Storage)?;
        if partition_type_matches_layer(&parent_snapshot.partition_type, &target_layer) {
            // Switch the staged pointer to this parent
            storage
                .update_pointer(&staged_pid, parent_id)
                .map_err(LayertwineError::Storage)?;
            return Ok(*parent_id);
        }
    }

    Err(LayertwineError::NotFound(format!(
        "no parent found for target layer {:?} in staged snapshot parents",
        target_layer
    )))
}

/// Unified rollback dispatch
///
/// Executes the rollback operation based on the `RollbackTransition` type.
/// This is the reverse counterpart to `execute_forward`.
pub fn execute_rollback<S>(
    storage: &S,
    transition: RollbackTransition,
    _params: &[&str],
) -> Result<SnapshotId>
where
    S: SnapshotStore + PartitionStore + DeltaStore + FileNodeStore + 'static,
{
    match transition {
        RollbackTransition::StagedToManual => {
            check_rollback_valid(&LayerType::Staged, &LayerType::ManualEdit)?;
            rollback_staged_to_layer(storage, LayerType::ManualEdit)
        }
        RollbackTransition::StagedToApproval => {
            check_rollback_valid(&LayerType::Staged, &LayerType::Approval)?;
            rollback_staged_to_layer(storage, LayerType::Approval)
        }
        RollbackTransition::StagedToAgentRaw => {
            check_rollback_valid(&LayerType::Staged, &LayerType::AgentEdit)?;
            rollback_staged_to_layer(storage, LayerType::AgentEdit)
        }
        RollbackTransition::ApprovalToAgentRaw => {
            check_rollback_valid(&LayerType::Approval, &LayerType::AgentEdit)?;
            let agent_id = _params.first().ok_or_else(|| {
                LayertwineError::StateMachine(
                    "ApprovalToAgentRaw requires agent_id parameter".into(),
                )
            })?;
            crate::layered::approval::reject_approval(
                storage,
                &crate::core::types::AgentInstanceId(agent_id.to_string()),
            )
        }
    }
}

/// Merge backup snapshots into staged
///
/// Uses BackupRepo to restore a backup into the staged partition.
pub fn merge_backup_to_staged<S>(
    storage: &S,
    backup_repo: &BackupRepo,
    backup_id: &BackupId,
) -> Result<SnapshotId>
where
    S: SnapshotStore + DeltaStore + FileNodeStore + PartitionStore,
{
    backup_repo.merge_to_staged(backup_id, storage)
}

// ===== Utility functions =====

/// Check if a partition_type string matches a LayerType via structural matching.
///
/// Uses `PartitionType::from_name()` to parse the string back into a proper enum,
/// then delegates to `PartitionType::to_layer()` for the canonical mapping.
/// This eliminates fragile string-prefix matching and ensures the same logic
/// that maps PartitionType → LayerType at creation time is used at query time.
pub fn partition_type_matches_layer(partition_type: &str, target_layer: &LayerType) -> bool {
    crate::core::types::PartitionType::from_name(partition_type)
        .map(|pt| pt.to_layer())
        .as_ref()
        == Some(target_layer)
}

/// Reconstructs the complete text content from Snapshot's delta chains
///
/// Read the original content from file_node and apply all deltas in turn.
/// apply_deltas now preserves trailing newlines, ensuring downstream
/// consumers (merge_texts, diff_to_line_diff) receive properly
/// line-terminated text for accurate TextDiff comparison.
///
/// Returns `Ok(None)` for snapshots carrying the explicit deletion marker
/// (`SnapshotContent::Deleted`): the file is missing, not cleared. Callers
/// that only need text (e.g. three-way merge inputs) treat `None` as an
/// empty string; callers that need the full semantics (restore, projection,
/// provenance) can distinguish "deleted" from "cleared".
pub fn reconstruct_text<S>(storage: &S, snapshot: &Snapshot) -> Result<Option<String>>
where
    S: FileNodeStore + DeltaStore + ?Sized,
{
    if snapshot.is_deleted() {
        return Ok(None);
    }
    let file_content = storage
        .get_file_content(snapshot.file.path_str(), &snapshot.file.base_hash)
        .map_err(LayertwineError::Storage)?;
    let content_str = String::from_utf8_lossy(&file_content).to_string();

    let deltas = storage
        .get_deltas(&snapshot.deltas)
        .map_err(LayertwineError::Storage)?;

    apply_deltas(&content_str, &deltas)
        .map(Some)
        .map_err(|e| LayertwineError::Engine(e.to_string()))
}

/// Checks if the snapshot contains a parent of the specified partition_type.
pub fn has_parent_of_type<S: SnapshotStore>(
    storage: &S,
    snapshot: &Snapshot,
    partition_type_prefix: &str,
) -> Result<bool> {
    for parent_id in &snapshot.parents {
        let parent = storage
            .get_snapshot(parent_id)
            .map_err(LayertwineError::Storage)?;
        if parent.partition_type.contains(partition_type_prefix) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::delta::Delta;
    use crate::core::file_node::FileNode;
    use crate::core::partition::Partition;
    use crate::core::types::{AgentInstanceId, PartitionType, SourceType};
    use crate::engine::diff::diff_to_line_diff;
    use crate::storage::repository::{DeltaStore, FileNodeStore, PartitionStore, SnapshotStore};
    use crate::test_utils::{create_initial_snapshot, setup_storage};
    use std::path::PathBuf;

    #[test]
    fn test_check_forward_valid() {
        // Permitted flows
        assert!(check_forward_valid(&LayerType::ManualEdit, &LayerType::Staged).is_ok());
        assert!(check_forward_valid(&LayerType::AgentEdit, &LayerType::Approval).is_ok());
        assert!(check_forward_valid(&LayerType::Approval, &LayerType::Integrated).is_ok());
        assert!(check_forward_valid(&LayerType::Integrated, &LayerType::Staged).is_ok());

        // Prohibition of cross-layering
        assert!(check_forward_valid(&LayerType::AgentEdit, &LayerType::Staged).is_err());
        assert!(check_forward_valid(&LayerType::ManualEdit, &LayerType::Approval).is_err());
        assert!(check_forward_valid(&LayerType::Approval, &LayerType::Staged).is_err());
    }

    #[test]
    fn test_check_forward_invalid_all() {
        let valid_pairs = [
            (LayerType::ManualEdit, LayerType::Staged),
            (LayerType::AgentEdit, LayerType::Approval),
            (LayerType::Approval, LayerType::Integrated),
            (LayerType::Integrated, LayerType::Staged),
        ];

        let all_pairs = [
            (LayerType::ManualEdit, LayerType::ManualEdit),
            (LayerType::ManualEdit, LayerType::AgentEdit),
            (LayerType::ManualEdit, LayerType::Approval),
            (LayerType::ManualEdit, LayerType::Integrated),
            (LayerType::ManualEdit, LayerType::Staged),
            (LayerType::AgentEdit, LayerType::ManualEdit),
            (LayerType::AgentEdit, LayerType::AgentEdit),
            (LayerType::AgentEdit, LayerType::Approval),
            (LayerType::AgentEdit, LayerType::Integrated),
            (LayerType::AgentEdit, LayerType::Staged),
            (LayerType::Approval, LayerType::ManualEdit),
            (LayerType::Approval, LayerType::AgentEdit),
            (LayerType::Approval, LayerType::Approval),
            (LayerType::Approval, LayerType::Integrated),
            (LayerType::Approval, LayerType::Staged),
            (LayerType::Integrated, LayerType::ManualEdit),
            (LayerType::Integrated, LayerType::AgentEdit),
            (LayerType::Integrated, LayerType::Approval),
            (LayerType::Integrated, LayerType::Integrated),
            (LayerType::Integrated, LayerType::Staged),
            (LayerType::Staged, LayerType::ManualEdit),
            (LayerType::Staged, LayerType::AgentEdit),
            (LayerType::Staged, LayerType::Approval),
            (LayerType::Staged, LayerType::Integrated),
            (LayerType::Staged, LayerType::Staged),
        ];

        for (from, to) in &all_pairs {
            let is_valid = valid_pairs.contains(&(from.clone(), to.clone()));
            let result = check_forward_valid(from, to);
            if is_valid {
                assert!(result.is_ok(), "expected OK for {:?} -> {:?}", from, to);
            } else {
                assert!(result.is_err(), "expected Err for {:?} -> {:?}", from, to);
            }
        }
    }

    #[test]
    fn test_check_rollback_valid() {
        assert!(check_rollback_valid(&LayerType::Staged, &LayerType::ManualEdit).is_ok());
        assert!(check_rollback_valid(&LayerType::Staged, &LayerType::AgentEdit).is_ok());
        assert!(check_rollback_valid(&LayerType::Staged, &LayerType::Approval).is_ok());
        assert!(check_rollback_valid(&LayerType::Approval, &LayerType::AgentEdit).is_ok());

        assert!(check_rollback_valid(&LayerType::Approval, &LayerType::Staged).is_err());
    }

    #[test]
    fn test_check_rollback_invalid_all() {
        let valid_pairs = [
            (LayerType::Staged, LayerType::ManualEdit),
            (LayerType::Staged, LayerType::AgentEdit),
            (LayerType::Staged, LayerType::Approval),
            (LayerType::Approval, LayerType::AgentEdit),
        ];

        let all_pairs = [
            (LayerType::ManualEdit, LayerType::ManualEdit),
            (LayerType::ManualEdit, LayerType::AgentEdit),
            (LayerType::ManualEdit, LayerType::Approval),
            (LayerType::ManualEdit, LayerType::Staged),
            (LayerType::AgentEdit, LayerType::ManualEdit),
            (LayerType::AgentEdit, LayerType::AgentEdit),
            (LayerType::AgentEdit, LayerType::Approval),
            (LayerType::AgentEdit, LayerType::Staged),
            (LayerType::Approval, LayerType::ManualEdit),
            (LayerType::Approval, LayerType::Approval),
            (LayerType::Approval, LayerType::Staged),
            (LayerType::Staged, LayerType::Staged),
        ];

        for (from, to) in &all_pairs {
            let is_valid = valid_pairs.contains(&(from.clone(), to.clone()));
            let result = check_rollback_valid(from, to);
            if is_valid {
                assert!(result.is_ok(), "expected OK for {:?} -> {:?}", from, to);
            } else {
                assert!(result.is_err(), "expected Err for {:?} -> {:?}", from, to);
            }
        }
    }

    #[test]
    fn test_rollback_partition() {
        let storage = setup_storage();
        let initial_id = create_initial_snapshot(&storage, "v1\n", SourceType::Manual);
        let pid = crate::layered::staged::staged_partition_id();
        let partition = Partition {
            id: pid,
            name: "test".into(),
            current_snapshot: initial_id,
            history: vec![initial_id],
            partition_type: PartitionType::Staged,
        };
        storage.create_partition(&partition).unwrap();

        // advance
        let file_node = FileNode::new(PathBuf::from("test.txt"), b"v1\n");
        let diff = crate::core::types::LineDiff::new(vec![]);
        let delta = Delta::new(file_node, diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();
        let snap = storage.get_snapshot(&initial_id).unwrap();
        let s2 = Snapshot::from_parent(&snap, delta.id, "staged".to_string());
        storage.store_snapshot(&s2, b"").unwrap();
        storage.update_pointer(&pid, &s2.id).unwrap();

        // rollback
        let prev = rollback_partition(&storage, &pid).unwrap();
        assert_eq!(prev, initial_id);
    }

    #[test]
    fn test_rollback_partition_error() {
        let storage = setup_storage();
        let initial_id = create_initial_snapshot(&storage, "v1\n", SourceType::Manual);
        let pid = crate::layered::staged::staged_partition_id();
        let partition = Partition {
            id: pid,
            name: "test".into(),
            current_snapshot: initial_id,
            history: vec![initial_id],
            partition_type: PartitionType::Staged,
        };
        storage.create_partition(&partition).unwrap();

        // Try rollback when history has only one entry
        let result = rollback_partition(&storage, &pid);
        assert!(
            result.is_err(),
            "should error when only one snapshot in history"
        );
    }

    #[test]
    fn test_reconstruct_text() {
        let storage = setup_storage();
        let file_node = FileNode::new(PathBuf::from("test.txt"), b"hello\n");
        storage.store_file_node(&file_node, b"hello\n").unwrap();

        // Create delta: modify "hello" to "hello world"
        let diff = crate::engine::diff::diff_to_line_diff("hello\n", "hello world\n");
        let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();

        let snapshot = Snapshot::new_initial(file_node, delta.id);
        storage.store_snapshot(&snapshot, b"").unwrap();

        let text = reconstruct_text(&storage, &snapshot).unwrap().unwrap();
        assert_eq!(text, "hello world\n");
    }

    #[test]
    fn test_execute_forward_manual_to_staged() {
        let storage = setup_storage();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);

        crate::layered::manual::ensure_manual_partition(&storage, initial_id, None).unwrap();
        crate::layered::staged::ensure_staged_partition(&storage, initial_id, None).unwrap();
        crate::layered::manual::apply_manual_edit(&storage, "test.txt", "base\nmodified\n", None)
            .unwrap();

        let result = execute_forward(&storage, ForwardTransition::ManualToStaged, &[]);
        assert!(result.is_ok());

        let staged = storage
            .get_partition(&crate::layered::staged::staged_partition_id())
            .unwrap();
        assert_ne!(staged.current_snapshot, initial_id);
    }

    #[test]
    fn test_has_parent_of_type() {
        let storage = setup_storage();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);

        let file_node = FileNode::new(PathBuf::from("test.txt"), b"base\nmodified\n");
        storage
            .store_file_node(&file_node, b"base\nmodified\n")
            .unwrap();
        let diff = diff_to_line_diff("base\n", "base\nmodified\n");
        let delta = Delta::new(file_node, diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();

        let parent_snap = storage.get_snapshot(&initial_id).unwrap();

        // Create a chain: parent_snap (type="") → manual_snap (type="manual_edit") → staged_snap (type="staged")
        let manual_snap = Snapshot::from_parent(&parent_snap, delta.id, "manual_edit".to_string());
        storage.store_snapshot(&manual_snap, b"").unwrap();

        let file_node2 = FileNode::new(PathBuf::from("test.txt"), b"base\n");
        storage.store_file_node(&file_node2, b"base\n").unwrap();
        let diff2 = diff_to_line_diff("base\nmodified\n", "base\n");
        let delta2 = Delta::new(file_node2, diff2, SourceType::Manual);
        storage.store_delta(&delta2).unwrap();
        let staged_snap = Snapshot::from_parent(&manual_snap, delta2.id, "staged".to_string());
        storage.store_snapshot(&staged_snap, b"").unwrap();

        // staged_snap has manual_snap as parent with "manual_edit" type
        let has_manual = has_parent_of_type(&storage, &staged_snap, "manual").unwrap();
        assert!(
            has_manual,
            "staged snapshot should have parent with 'manual'"
        );

        // Initial snapshot (no partition type) should not match
        let has = has_parent_of_type(&storage, &parent_snap, "manual").unwrap();
        assert!(!has, "initial snapshot has no parent");
    }

    #[test]
    fn test_execute_forward_agent_to_approval() {
        let storage = setup_storage();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        let agent_id = AgentInstanceId("test-agent".into());

        // Setup agent partition
        crate::layered::agent::ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();
        crate::layered::agent::apply_agent_edit(
            &storage,
            &agent_id,
            "test.txt",
            "base\nmodified\n",
        )
        .unwrap();

        // Setup approval partition
        let approval_pid = crate::layered::approval::approval_agent_partition_id(&agent_id);
        let approval_part = Partition {
            id: approval_pid,
            name: format!("approval/{}", agent_id),
            current_snapshot: initial_id,
            history: vec![initial_id],
            partition_type: PartitionType::Approval(agent_id.clone()),
        };
        storage.create_partition(&approval_part).unwrap();

        let result = execute_forward(
            &storage,
            ForwardTransition::AgentToApproval,
            &["test-agent"],
        );
        assert!(result.is_ok());

        let approval = storage.get_partition(&approval_pid).unwrap();
        assert_ne!(approval.current_snapshot, initial_id);
    }

    #[test]
    fn test_rollback_staged_to_layer_not_found() {
        let storage = setup_storage();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);
        crate::layered::staged::ensure_staged_partition(&storage, initial_id, None).unwrap();

        let result = rollback_staged_to_layer(&storage, LayerType::ManualEdit);
        assert!(
            result.is_err(),
            "should error when no suitable parent found"
        );
    }

    #[test]
    fn test_execute_rollback_staged_to_manual() {
        let storage = setup_storage();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);

        crate::layered::manual::ensure_manual_partition(&storage, initial_id, None).unwrap();
        crate::layered::staged::ensure_staged_partition(&storage, initial_id, None).unwrap();
        crate::layered::manual::apply_manual_edit(&storage, "test.txt", "base\nmodified\n", None)
            .unwrap();
        crate::layered::manual::merge_manual_to_staged(&storage, None).unwrap();

        // Verify staged has changed
        let staged_pid = crate::layered::staged::staged_partition_id();
        let staged_before = storage.get_partition(&staged_pid).unwrap();
        assert_ne!(staged_before.current_snapshot, initial_id);

        // Rollback staged → manual
        let result = execute_rollback(&storage, RollbackTransition::StagedToManual, &[]);
        assert!(result.is_ok());

        let staged_after = storage.get_partition(&staged_pid).unwrap();
        // Staged should roll back to the manual partition's snapshot (not the initial)
        let manual_pid = crate::layered::manual::manual_partition_id();
        let manual_partition = storage.get_partition(&manual_pid).unwrap();
        assert_eq!(
            staged_after.current_snapshot,
            manual_partition.current_snapshot
        );
    }
}
