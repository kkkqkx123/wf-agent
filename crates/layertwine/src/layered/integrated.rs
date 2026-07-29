//! Integrated partition operations
//!
//! Manages named Integrated (feature) partitions. Each feature gets its own partition
//! where multiple agents' approved changes are merged via three-way merge.
//! Flow: approval_agent → integrated → unified → staged.

use crate::core::delta::Delta;
use crate::core::partition::Partition;
use crate::core::snapshot::Snapshot;
use crate::core::types::{AgentInstanceId, PartitionId, PartitionType, SnapshotId, SourceType};
use crate::engine::diff::diff_to_line_diff;
use crate::error::{LayertwineError, Result};
use crate::layered::MergeResult;
use crate::storage::repository::{DeltaStore, FileNodeStore, PartitionStore, SnapshotStore};

// Partition ID generation -

/// ID of the Integrated partition for the given name via UUIDv5
pub fn integrated_partition_id(name: &str) -> PartitionId {
    let namespace = uuid::Uuid::from_u128(0x4000_0000_0000_0000_0000_0000_0000_0000);
    uuid::Uuid::new_v5(&namespace, name.as_bytes())
}

// Partition creation -

/// Get or create an Integrated partition
pub fn ensure_integrated_partition<S: PartitionStore>(
    storage: &S,
    name: &str,
    initial_snapshot_id: SnapshotId,
) -> Result<Partition> {
    let pid = integrated_partition_id(name);
    match storage.get_partition(&pid) {
        Ok(p) => Ok(p),
        Err(_) => {
            let partition = Partition {
                id: pid,
                name: format!("integrated/{}", name),
                current_snapshot: initial_snapshot_id,
                history: vec![initial_snapshot_id],
                partition_type: PartitionType::Integrated(name.to_string()),
            };
            storage
                .create_partition(&partition)
                .map_err(LayertwineError::Storage)?;
            Ok(partition)
        }
    }
}

/// Create a new feature branch with explicit baseline
/// This makes the baseline concept clear for feature development
pub fn create_feature_branch<S: PartitionStore>(
    storage: &S,
    name: &str,
    baseline_snapshot_id: SnapshotId,
) -> Result<Partition> {
    let pid = integrated_partition_id(name);
    let partition = Partition {
        id: pid,
        name: format!("feature/{}", name),
        current_snapshot: baseline_snapshot_id,
        history: vec![baseline_snapshot_id],
        partition_type: PartitionType::Integrated(name.to_string()),
    };
    storage
        .create_partition(&partition)
        .map_err(LayertwineError::Storage)?;
    Ok(partition)
}

/// Get the baseline snapshot for a feature branch
/// The baseline is the first snapshot in the feature's history
pub fn get_feature_baseline<S: SnapshotStore + PartitionStore>(
    storage: &S,
    feature_name: &str,
) -> Result<Snapshot> {
    let pid = integrated_partition_id(feature_name);
    let part = storage.get_partition(&pid).map_err(|_| {
        LayertwineError::NotFound(format!("integrated partition {} not found", feature_name))
    })?;
    let baseline_id = &part.history[0];
    storage
        .get_snapshot(baseline_id)
        .map_err(LayertwineError::Storage)
}

// Forward migration operations -

/// Merge an Agent's approval into a feature branch using three-way merge
/// This supports multiple agents collaborating on the same feature
pub fn merge_agent_to_feature<S>(
    storage: &S,
    agent_id: &AgentInstanceId,
    feature_name: &str,
) -> Result<MergeResult>
where
    S: SnapshotStore + DeltaStore + FileNodeStore + PartitionStore,
{
    let approval_pid = crate::layered::approval::approval_agent_partition_id(agent_id);
    let integrated_pid = integrated_partition_id(feature_name);

    let approval_partition = storage.get_partition(&approval_pid).map_err(|_| {
        LayertwineError::NotFound(format!("approval agent partition {} not found", agent_id))
    })?;
    let approval_snapshot = storage
        .get_snapshot(&approval_partition.current_snapshot)
        .map_err(LayertwineError::Storage)?;

    // Use the approval partition's original baseline (history[0]) as the
    // integrated partition's initial snapshot. This ensures the merge baseline
    // is the common ancestor — the staged snapshot at the time of agent submission.
    // Using approval_partition.current_snapshot would set the baseline to the
    // post-edit content, causing downstream three-way merges to collapse to
    // "ours wins" since baseline == theirs, which means staged never advances.
    let approval_baseline = approval_partition.history.first().ok_or_else(|| {
        LayertwineError::StateMachine("approval partition has empty history".into())
    })?;
    let integrated_partition =
        ensure_integrated_partition(storage, feature_name, *approval_baseline)?;

    if integrated_partition.current_snapshot == approval_partition.current_snapshot {
        return Ok(MergeResult {
            snapshot_id: integrated_partition.current_snapshot,
            conflicts: vec![],
        });
    }

    let baseline_snapshot = get_feature_baseline(storage, feature_name)?;
    let integrated_snapshot = storage
        .get_snapshot(&integrated_partition.current_snapshot)
        .map_err(LayertwineError::Storage)?;

    let baseline_text = crate::layered::transition::reconstruct_text(storage, &baseline_snapshot)?;
    let approval_text = crate::layered::transition::reconstruct_text(storage, &approval_snapshot)?;
    let integrated_text =
        crate::layered::transition::reconstruct_text(storage, &integrated_snapshot)?;

    let (merged_text, conflicts) =
        crate::engine::merge::merge_texts(&baseline_text, &approval_text, &integrated_text);

    let has_conflicts = !conflicts.is_empty();

    let merge_diff = diff_to_line_diff(&integrated_text, &merged_text);
    if merge_diff.is_empty() {
        return Ok(MergeResult {
            snapshot_id: integrated_partition.current_snapshot,
            conflicts,
        });
    }

    let approval_deltas = storage
        .get_deltas(&approval_snapshot.deltas)
        .map_err(LayertwineError::Storage)?;
    let merge_file = approval_deltas
        .last()
        .map(|d| d.file.clone())
        .unwrap_or_else(|| integrated_snapshot.file.clone());
    let merge_delta = Delta::new(merge_file, merge_diff, SourceType::Agent(agent_id.clone()));
    storage
        .store_delta(&merge_delta)
        .map_err(LayertwineError::Storage)?;

    let new_snapshot = Snapshot::merge(
        vec![&integrated_snapshot, &approval_snapshot, &baseline_snapshot],
        merge_delta.id,
        PartitionType::Integrated(feature_name.to_string()).name(),
        has_conflicts,
    );
    storage
        .store_snapshot(&new_snapshot, b"")
        .map_err(LayertwineError::Storage)?;

    storage
        .update_pointer(&integrated_pid, &new_snapshot.id)
        .map_err(LayertwineError::Storage)?;

    // Reset the approval partition back to baseline after successful merge.
    // This ensures list_pending_approvals() (which checks history.len() > 1)
    // correctly identifies only truly pending approvals — those that have been
    // submitted but not yet merged into the integrated feature.
    crate::layered::approval::reject_approval(storage, agent_id)?;

    Ok(MergeResult {
        snapshot_id: new_snapshot.id,
        conflicts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::SourceType;
    use crate::test_utils::{create_initial_snapshot, setup_storage};

    #[test]
    fn test_ensure_integrated_partition() {
        let storage = setup_storage();
        let initial_id = create_initial_snapshot(&storage, "base\n", SourceType::Manual);

        let p1 = ensure_integrated_partition(&storage, "feat-1", initial_id).unwrap();
        let p2 = ensure_integrated_partition(&storage, "feat-1", initial_id).unwrap();
        assert_eq!(p1.id, p2.id);

        let p3 = ensure_integrated_partition(&storage, "feat-2", initial_id).unwrap();
        assert_ne!(
            p1.id, p3.id,
            "different names should produce different partition ids"
        );
    }

    #[test]
    fn test_partition_id_uniqueness() {
        let agent_a = AgentInstanceId("agent-a".into());
        let agent_b = AgentInstanceId("agent-b".into());

        let aa = crate::layered::approval::approval_agent_partition_id(&agent_a);
        let ab = crate::layered::approval::approval_agent_partition_id(&agent_b);
        assert_ne!(
            aa, ab,
            "different agents should have different approval partition ids"
        );

        let ia = integrated_partition_id("feat-a");
        let ib = integrated_partition_id("feat-b");
        assert_ne!(
            ia, ib,
            "different integrations should have different partition ids"
        );

        assert_ne!(
            aa, ia,
            "approval and integrated partition ids should differ"
        );
    }
}
