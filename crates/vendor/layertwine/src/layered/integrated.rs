//! Integrated partition operations
//!
//! Manages named Integrated (feature) partitions. Each feature gets its own partition
//! where multiple agents' approved changes are merged via three-way merge.
//! Flow: approval_agent → integrated → unified → staged.

use crate::core::delta::Delta;
use crate::core::partition::Partition;
use crate::core::snapshot::{Snapshot, SnapshotContent};
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
                redo_stack: Vec::new(),
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
        redo_stack: Vec::new(),
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

/// Resolve the file path a snapshot applies to.
fn snapshot_path<S>(storage: &S, snapshot: &Snapshot) -> Result<String>
where
    S: DeltaStore,
{
    if let Some(delta_id) = snapshot.deltas.last() {
        let delta = storage
            .get_delta(delta_id)
            .map_err(LayertwineError::Storage)?;
        Ok(delta.file.path_str().to_string())
    } else {
        Ok(snapshot.file.path_str().to_string())
    }
}

/// Latest snapshot per file path in a partition history (last occurrence wins).
fn latest_per_path<S>(
    storage: &S,
    history: &[SnapshotId],
) -> Result<std::collections::BTreeMap<String, Snapshot>>
where
    S: SnapshotStore + DeltaStore,
{
    let mut map = std::collections::BTreeMap::new();
    for snapshot_id in history {
        let snapshot = storage
            .get_snapshot(snapshot_id)
            .map_err(LayertwineError::Storage)?;
        let path = snapshot_path(storage, &snapshot)?;
        if path == ".wf-checkpoint-seed" {
            continue;
        }
        map.insert(path, snapshot);
    }
    Ok(map)
}

fn snapshot_is_binary(snapshot: &Snapshot) -> bool {
    match &snapshot.content {
        Some(SnapshotContent::FileContent(bytes)) => std::str::from_utf8(bytes).is_err(),
        _ => false,
    }
}

/// Merge an Agent's approval into a feature branch using three-way merge.
///
/// Advances the feature partition per file path instead of merging only the
/// approval head snapshot: every submitted path is merged (binary payloads
/// and deletions bypass the text merge verbatim) and appended sequentially.
/// This supports multiple agents collaborating on the same feature.
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

    let approval_latest = latest_per_path(storage, &approval_partition.history)?;
    if approval_latest.is_empty() {
        return Ok(MergeResult {
            snapshot_id: integrated_partition.current_snapshot,
            conflicts: vec![],
        });
    }
    let baseline_snapshot = get_feature_baseline(storage, feature_name)?;
    let mut integrated_latest =
        latest_per_path(storage, &integrated_partition.history)?;
    let baseline_path = snapshot_path(storage, &baseline_snapshot).unwrap_or_default();
    let baseline_text =
        crate::layered::transition::reconstruct_text(storage, &baseline_snapshot)?
            .unwrap_or_default();

    let mut head_id = integrated_partition.current_snapshot;
    let mut all_conflicts = Vec::new();
    for (path, approval_snapshot) in &approval_latest {
        let integrated_snapshot_opt = integrated_latest.get(path);
        // Idempotency across partitions: compare content state, not snapshot
        // IDs (merge copies always differ in ID even for identical content).
        if let Some(integrated_snapshot) = integrated_snapshot_opt {
            let both_deleted =
                integrated_snapshot.is_deleted() && approval_snapshot.is_deleted();
            if both_deleted {
                continue;
            }
            if !integrated_snapshot.is_deleted()
                && !approval_snapshot.is_deleted()
                && !snapshot_is_binary(integrated_snapshot)
                && !snapshot_is_binary(approval_snapshot)
            {
                let integrated_text = crate::layered::transition::reconstruct_text(
                    storage,
                    integrated_snapshot,
                )?
                .unwrap_or_default();
                let approval_text = crate::layered::transition::reconstruct_text(
                    storage,
                    approval_snapshot,
                )?
                .unwrap_or_default();
                if integrated_text == approval_text {
                    continue;
                }
            }
        }
        let head_snapshot = storage
            .get_snapshot(&head_id)
            .map_err(LayertwineError::Storage)?;

        // Binary payloads and deletions bypass the text merge: adopt the
        // approval bytes verbatim so arbitrary byte sequences survive.
        if approval_snapshot.is_deleted() || snapshot_is_binary(approval_snapshot) {
            let content = if approval_snapshot.is_deleted() {
                SnapshotContent::Deleted
            } else if let Some(SnapshotContent::FileContent(bytes)) = &approval_snapshot.content
            {
                SnapshotContent::FileContent(bytes.clone())
            } else {
                SnapshotContent::Deleted
            };
            let bytes = match &content {
                SnapshotContent::FileContent(b) => b.clone(),
                _ => Vec::new(),
            };
            let snapshot = Snapshot::new_with_content(
                approval_snapshot.file.clone(),
                content,
                approval_snapshot.source.clone(),
                PartitionType::Integrated(feature_name.to_string()).name(),
                vec![head_snapshot.id, approval_snapshot.id],
                vec![],
            );
            storage
                .store_snapshot(&snapshot, &bytes)
                .map_err(LayertwineError::Storage)?;
            storage
                .update_pointer(&integrated_pid, &snapshot.id)
                .map_err(LayertwineError::Storage)?;
            head_id = snapshot.id;
            let stored = storage
                .get_snapshot(&head_id)
                .map_err(LayertwineError::Storage)?;
            integrated_latest.insert(path.clone(), stored);
            continue;
        }

        let approval_text =
            crate::layered::transition::reconstruct_text(storage, approval_snapshot)?
                .unwrap_or_default();
        let integrated_text = match integrated_snapshot_opt {
            Some(snap) => {
                crate::layered::transition::reconstruct_text(storage, snap)?.unwrap_or_default()
            }
            None => String::new(),
        };

        // Per-path base: the feature baseline text when it applies to this
        // path, otherwise empty (path created after the baseline).
        let base_text = if baseline_path == *path {
            baseline_text.clone()
        } else {
            String::new()
        };
        let (merged_text, conflicts) =
            crate::engine::merge::merge_texts(&base_text, &approval_text, &integrated_text);
        let has_conflicts = !conflicts.is_empty();

        // Linear transform chain: the delta must transform the actual head
        // text (previous path in this batch) into the merged text.
        let head_text =
            crate::layered::transition::reconstruct_text(storage, &head_snapshot)?
                .unwrap_or_default();
        let merge_diff = diff_to_line_diff(&head_text, &merged_text);
        if merge_diff.is_empty() {
            all_conflicts.extend(conflicts);
            continue;
        }

        // The merge delta must carry the current path: snapshot `file`
        // fields are parent-inherited (stale for delta-chain snapshots).
        let merge_file = crate::core::file_node::FileNode::new(
            std::path::PathBuf::from(path),
            head_text.as_bytes(),
        );
        let merge_delta = Delta::new(merge_file, merge_diff, SourceType::Agent(agent_id.clone()));
        storage
            .store_delta(&merge_delta)
            .map_err(LayertwineError::Storage)?;

        let new_snapshot = Snapshot::merge(
            vec![&head_snapshot, approval_snapshot, &baseline_snapshot],
            merge_delta.id,
            PartitionType::Integrated(feature_name.to_string()).name(),
            has_conflicts,
        )?;
        storage
            .store_snapshot(&new_snapshot, b"")
            .map_err(LayertwineError::Storage)?;

        storage
            .update_pointer(&integrated_pid, &new_snapshot.id)
            .map_err(LayertwineError::Storage)?;
        head_id = new_snapshot.id;
        all_conflicts.extend(conflicts);
        let stored = storage
            .get_snapshot(&head_id)
            .map_err(LayertwineError::Storage)?;
        integrated_latest.insert(path.clone(), stored);
    }

    // Reset the approval partition back to baseline after successful merge.
    // This ensures list_pending_approvals() (which checks history.len() > 1)
    // correctly identifies only truly pending approvals — those that have been
    // submitted but not yet merged into the integrated feature.
    crate::layered::approval::reject_approval(storage, agent_id)?;

    Ok(MergeResult {
        snapshot_id: head_id,
        conflicts: all_conflicts,
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
