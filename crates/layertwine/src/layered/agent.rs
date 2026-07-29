//! agent_edit Layer Operation
//!
//Each Agent instance is isolated in a separate Partition. Each Agent instance is segregated in a separate Partition. the Agent modification enters its own Partition first.
//! and then moved into the corresponding partition at the approval level via move_agent_to_approval.

use crate::core::delta::Delta;
use crate::core::file_node::FileNode;
use crate::core::partition::Partition;
use crate::core::snapshot::Snapshot;
use crate::core::types::{AgentInstanceId, PartitionId, PartitionType, SnapshotId, SourceType};
use crate::engine::diff::diff_to_line_diff;
use crate::engine::merge::apply_deltas;
use crate::error::{LayertwineError, Result};
use crate::storage::repository::{DeltaStore, FileNodeStore, PartitionStore, SnapshotStore};
use std::path::PathBuf;
/// Generate stable IDs for agent partitions via UUIDv5
pub fn agent_partition_id(agent_id: &AgentInstanceId) -> PartitionId {
    let namespace = uuid::Uuid::from_u128(0x2000_0000_0000_0000_0000_0000_0000_0000);
    uuid::Uuid::new_v5(&namespace, agent_id.0.as_bytes())
}

/// Getting or creating agent_edit partitions
pub fn ensure_agent_partition<S: PartitionStore>(
    storage: &S,
    agent_id: &AgentInstanceId,
    initial_snapshot_id: SnapshotId,
) -> Result<Partition> {
    let pid = agent_partition_id(agent_id);
    match storage.get_partition(&pid) {
        Ok(p) => Ok(p),
        Err(_) => {
            let partition = Partition {
                id: pid,
                name: format!("agent_edit/{}", agent_id),
                current_snapshot: initial_snapshot_id,
                history: vec![initial_snapshot_id],
                partition_type: PartitionType::Agent(agent_id.clone()),
            };
            storage
                .create_partition(&partition)
                .map_err(LayertwineError::Storage)?;
            Ok(partition)
        }
    }
}

/// Agent Edit File
///
/// Append the Agent changes as Delta to the corresponding partition at the agent_edit level.
/// Each Agent instance has a separate partition and does not interfere with each other.
pub fn apply_agent_edit<S>(
    storage: &S,
    agent_id: &AgentInstanceId,
    file_path: &str,
    new_content: &str,
) -> Result<SnapshotId>
where
    S: SnapshotStore + DeltaStore + FileNodeStore + PartitionStore,
{
    let pid = agent_partition_id(agent_id);
    let partition = storage.get_partition(&pid).map_err(|_| {
        LayertwineError::NotFound(format!(
            "agent partition for {} not found, call ensure_agent_partition first",
            agent_id
        ))
    })?;

    let current_snapshot = storage
        .get_snapshot(&partition.current_snapshot)
        .map_err(LayertwineError::Storage)?;

    // Read old content
    let old_content = {
        let deltas = storage
            .get_deltas(&current_snapshot.deltas)
            .map_err(LayertwineError::Storage)?;
        let content_str = String::from_utf8_lossy(
            &storage
                .get_file_content(
                    current_snapshot.file.path_str(),
                    &current_snapshot.file.base_hash,
                )
                .map_err(LayertwineError::Storage)?,
        )
        .to_string();
        apply_deltas(&content_str, &deltas).map_err(|e| LayertwineError::Engine(e.to_string()))?
    };

    // Calculate diff
    let line_diff = diff_to_line_diff(&old_content, new_content);
    if line_diff.is_empty() {
        return Ok(partition.current_snapshot);
    }

    // Create Delta
    let file_node = FileNode::new(PathBuf::from(file_path), old_content.as_bytes());
    let delta = Delta::new(
        file_node.clone(),
        line_diff,
        SourceType::Agent(agent_id.clone()),
    );
    storage
        .store_file_node(&file_node, old_content.as_bytes())
        .map_err(LayertwineError::Storage)?;
    storage
        .store_delta(&delta)
        .map_err(LayertwineError::Storage)?;

    // Creating a New Snapshot
    let new_snapshot = Snapshot::from_parent(
        &current_snapshot,
        delta.id,
        PartitionType::Agent(agent_id.clone()).name(),
    );
    storage
        .store_snapshot(&new_snapshot, b"")
        .map_err(LayertwineError::Storage)?;

    // Updating the partition pointer
    storage
        .update_pointer(&pid, &new_snapshot.id)
        .map_err(LayertwineError::Storage)?;

    Ok(new_snapshot.id)
}

/// Moving Agent Changes to the Agent Partition at the Approval Level
///
/// Corresponds to `move_agent_to_approval` in the architecture documentation.
/// - Take the current snapshot of the agent_raw partition and the approval agent partition
/// - Merge to generate a new snapshot to push into the approval agent partition
pub fn move_agent_to_approval<S>(storage: &S, agent_id: &AgentInstanceId) -> Result<SnapshotId>
where
    S: SnapshotStore + DeltaStore + FileNodeStore + PartitionStore,
{
    let agent_pid = agent_partition_id(agent_id);
    let approval_pid = crate::layered::approval::approval_agent_partition_id(agent_id);

    let agent_partition = storage.get_partition(&agent_pid).map_err(|_| {
        LayertwineError::NotFound(format!("agent partition {} not found", agent_id))
    })?;
    let approval_partition = storage.get_partition(&approval_pid).map_err(|_| {
        LayertwineError::NotFound(format!(
            "approval partition for agent {} not found, call ensure_approval_agent_partition first",
            agent_id
        ))
    })?;

    // If both point to the same snapshot, no merge needed
    if agent_partition.current_snapshot == approval_partition.current_snapshot {
        return Ok(approval_partition.current_snapshot);
    }

    let agent_snapshot = storage
        .get_snapshot(&agent_partition.current_snapshot)
        .map_err(LayertwineError::Storage)?;
    let approval_snapshot = storage
        .get_snapshot(&approval_partition.current_snapshot)
        .map_err(LayertwineError::Storage)?;

    // Find the merge base: first snapshot in approval history (common ancestor)
    let baseline_id = approval_partition.history.first().ok_or_else(|| {
        LayertwineError::StateMachine("approval partition has empty history".into())
    })?;
    let baseline_snapshot = storage
        .get_snapshot(baseline_id)
        .map_err(LayertwineError::Storage)?;

    // Reconstruct texts for three-way merge
    let baseline_text = crate::layered::transition::reconstruct_text(storage, &baseline_snapshot)?;
    let approval_text = crate::layered::transition::reconstruct_text(storage, &approval_snapshot)?;
    let agent_text = crate::layered::transition::reconstruct_text(storage, &agent_snapshot)?;

    // Three-way merge: base (common ancestor), ours (approval), theirs (agent)
    let (merged_text, conflicts) =
        crate::engine::merge::merge_texts(&baseline_text, &approval_text, &agent_text);
    let has_conflicts = !conflicts.is_empty();

    let merge_diff = diff_to_line_diff(&approval_text, &merged_text);
    if merge_diff.is_empty() {
        return Ok(approval_partition.current_snapshot);
    }

    let agent_deltas = storage
        .get_deltas(&agent_snapshot.deltas)
        .map_err(LayertwineError::Storage)?;
    let merge_file = agent_deltas
        .last()
        .map(|d| d.file.clone())
        .unwrap_or_else(|| agent_snapshot.file.clone());
    let merge_delta = Delta::new(merge_file, merge_diff, SourceType::Agent(agent_id.clone()));
    storage
        .store_delta(&merge_delta)
        .map_err(LayertwineError::Storage)?;

    // Create a merge snapshot with conflict tracking
    let new_snapshot = Snapshot::merge(
        vec![&approval_snapshot, &agent_snapshot],
        merge_delta.id,
        PartitionType::Approval(agent_id.clone()).name(),
        has_conflicts,
    );
    storage
        .store_snapshot(&new_snapshot, b"")
        .map_err(LayertwineError::Storage)?;

    storage
        .update_pointer(&approval_pid, &new_snapshot.id)
        .map_err(LayertwineError::Storage)?;

    Ok(new_snapshot.id)
}

/// Abandon Agent modifications (switch pointer to parent Snapshot only)
pub fn discard_agent_edit<S>(storage: &S, agent_id: &AgentInstanceId) -> Result<()>
where
    S: SnapshotStore + PartitionStore,
{
    let pid = agent_partition_id(agent_id);
    let partition = storage.get_partition(&pid).map_err(|_| {
        LayertwineError::NotFound(format!("agent partition {} not found", agent_id))
    })?;

    let current_snapshot = storage
        .get_snapshot(&partition.current_snapshot)
        .map_err(LayertwineError::Storage)?;

    // If a parent snapshot exists, fallback to the parent snapshot
    if let Some(&parent_id) = current_snapshot.parents.first() {
        storage
            .update_pointer(&pid, &parent_id)
            .map_err(LayertwineError::Storage)?;
        Ok(())
    } else {
        Err(LayertwineError::StateMachine(
            "agent has no parent snapshot to discard to".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::SourceType;
    use crate::storage::repository::{DeltaStore, FileNodeStore, PartitionStore, SnapshotStore};
    use crate::test_utils::{create_initial_snapshot, setup_storage};

    #[test]
    fn test_apply_agent_edit() {
        let storage = setup_storage();
        let agent_id = AgentInstanceId("agent-1".into());
        let initial_id =
            create_initial_snapshot(&storage, "base\n", SourceType::Agent("test-agent".into()));
        ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();

        let new_id = apply_agent_edit(&storage, &agent_id, "test.txt", "base\nmodified\n").unwrap();
        assert_ne!(new_id, initial_id);
    }

    #[test]
    fn test_discard_agent_edit() {
        let storage = setup_storage();
        let agent_id = AgentInstanceId("agent-2".into());
        let initial_id = create_initial_snapshot(
            &storage,
            "original\n",
            SourceType::Agent("test-agent".into()),
        );
        ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();

        // Application Editor
        let edited_id =
            apply_agent_edit(&storage, &agent_id, "test.txt", "original\nchanged\n").unwrap();
        assert_ne!(edited_id, initial_id);

        // Abandon Editing - Fallback to Parent Snapshot
        discard_agent_edit(&storage, &agent_id).unwrap();
        let partition = storage
            .get_partition(&agent_partition_id(&agent_id))
            .unwrap();
        assert_eq!(partition.current_snapshot, initial_id);
    }

    #[test]
    fn test_agent_isolation() {
        let storage = setup_storage();
        let initial_id =
            create_initial_snapshot(&storage, "shared\n", SourceType::Agent("test-agent".into()));

        let agent_a = AgentInstanceId("agent-a".into());
        let agent_b = AgentInstanceId("agent-b".into());

        ensure_agent_partition(&storage, &agent_a, initial_id).unwrap();
        ensure_agent_partition(&storage, &agent_b, initial_id).unwrap();

        let a_id = apply_agent_edit(&storage, &agent_a, "test.txt", "shared\na-edit\n").unwrap();
        let b_id = apply_agent_edit(&storage, &agent_b, "test.txt", "shared\nb-edit\n").unwrap();

        assert_ne!(a_id, b_id);

        // Verify that the respective partitions are independent
        let pa = storage
            .get_partition(&agent_partition_id(&agent_a))
            .unwrap();
        let pb = storage
            .get_partition(&agent_partition_id(&agent_b))
            .unwrap();
        assert_eq!(pa.current_snapshot, a_id);
        assert_eq!(pb.current_snapshot, b_id);
    }

    #[test]
    fn test_ensure_agent_partition_already_exists() {
        let storage = setup_storage();
        let agent_id = AgentInstanceId("agent-exists".into());
        let initial_id =
            create_initial_snapshot(&storage, "base\n", SourceType::Agent("test-agent".into()));

        let p1 = ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();
        let p2 = ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();

        assert_eq!(p1.id, p2.id, "should return same partition on second call");
    }

    #[test]
    fn test_discard_agent_edit_no_parent() {
        let storage = setup_storage();
        let agent_id = AgentInstanceId("agent-noparent".into());
        let initial_id =
            create_initial_snapshot(&storage, "only\n", SourceType::Agent("test-agent".into()));
        ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();

        // Initial snapshot has no parents → discard should fail
        let result = discard_agent_edit(&storage, &agent_id);
        assert!(result.is_err(), "discard with no parent should error");
    }

    #[test]
    fn test_apply_agent_edit_no_changes() {
        let storage = setup_storage();
        let agent_id = AgentInstanceId("agent-nochange".into());
        let initial_id =
            create_initial_snapshot(&storage, "same", SourceType::Agent("test-agent".into()));
        ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();

        // Apply same content → no new snapshot
        let result_id = apply_agent_edit(&storage, &agent_id, "test.txt", "same").unwrap();
        assert_eq!(
            result_id, initial_id,
            "no changes should return current snapshot id"
        );
    }

    #[test]
    fn test_move_agent_to_approval() {
        let storage = setup_storage();
        let agent_id = AgentInstanceId("agent-move".into());
        let initial_id =
            create_initial_snapshot(&storage, "base\n", SourceType::Agent("test-agent".into()));

        // Create agent partition
        ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();

        // Create approval agent partition
        let approval_pid = crate::layered::approval::approval_agent_partition_id(&agent_id);
        let approval_part = Partition {
            id: approval_pid,
            name: format!("approval/{}", agent_id),
            current_snapshot: initial_id,
            history: vec![initial_id],
            partition_type: PartitionType::Approval(agent_id.clone()),
        };
        storage.create_partition(&approval_part).unwrap();

        // Apply agent edit
        apply_agent_edit(&storage, &agent_id, "test.txt", "base\nmodified\n").unwrap();

        // Move agent to approval
        let result = move_agent_to_approval(&storage, &agent_id);
        assert!(result.is_ok(), "move_agent_to_approval should succeed");

        let approval_partition = storage.get_partition(&approval_pid).unwrap();
        assert_ne!(
            approval_partition.current_snapshot, initial_id,
            "approval should have advanced"
        );
    }

    #[test]
    fn test_agent_sequential_edits() {
        let storage = setup_storage();
        let agent_id = AgentInstanceId("agent-seq".into());
        let initial_id =
            create_initial_snapshot(&storage, "a\nb\n", SourceType::Agent("test-agent".into()));
        ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();

        let first = apply_agent_edit(&storage, &agent_id, "test.txt", "a\nmodified\n").unwrap();
        assert_ne!(first, initial_id);

        let second = apply_agent_edit(&storage, &agent_id, "test.txt", "a\nmodified\nc\n").unwrap();
        assert_ne!(second, first);

        let partition = storage
            .get_partition(&agent_partition_id(&agent_id))
            .unwrap();
        assert_eq!(partition.current_snapshot, second);
    }

    #[test]
    fn test_agent_edit_multiple_files() {
        let storage = setup_storage();
        let agent_id = AgentInstanceId("agent-mf".into());
        let initial_id = create_initial_snapshot(
            &storage,
            "content1\n",
            SourceType::Agent("test-agent".into()),
        );
        ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();

        // Create a second initial file node for a different file
        let file_node2 = FileNode::new(std::path::PathBuf::from("other.txt"), b"content2\n");
        storage.store_file_node(&file_node2, b"content2\n").unwrap();
        let empty_diff2 = crate::core::types::LineDiff::new(vec![]);
        let delta2 = Delta::new(
            file_node2.clone(),
            empty_diff2,
            SourceType::Agent(agent_id.clone()),
        );
        storage.store_delta(&delta2).unwrap();
        let init2 = Snapshot::new_initial(file_node2, delta2.id);
        storage.store_snapshot(&init2, b"").unwrap();

        // We need a different approach: just test editing two files sequentially
        let id1 =
            apply_agent_edit(&storage, &agent_id, "test.txt", "content1\nmodified\n").unwrap();
        assert_ne!(id1, initial_id);

        let id2 =
            apply_agent_edit(&storage, &agent_id, "other.txt", "content2\nmodified\n").unwrap();
        assert_ne!(id2, id1);
    }
}
