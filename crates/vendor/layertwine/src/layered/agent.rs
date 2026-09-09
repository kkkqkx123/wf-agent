//! agent_edit Layer Operation
//!
//Each Agent instance is isolated in a separate Partition. Each Agent instance is segregated in a separate Partition. the Agent modification enters its own Partition first.
//! and then moved into the corresponding partition at the approval level via move_agent_to_approval.

use crate::core::delta::Delta;
use crate::core::file_node::FileNode;
use crate::core::partition::Partition;
use crate::core::snapshot::{Snapshot, SnapshotContent};
use crate::core::types::{
    AgentInstanceId, EditSessionId, PartitionId, PartitionType, SnapshotId, SourceType,
};
use crate::engine::diff::{diff_to_line_diff, should_use_full_snapshot_content};
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
                redo_stack: Vec::new(),
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
    apply_agent_edit_with_session(storage, agent_id, file_path, new_content, None)
}

/// Agent edit with optional edit session grouping.
///
/// When `session_id` is provided, the created delta is associated with
/// the session, enabling atomic rollback of the entire logical operation.
pub fn apply_agent_edit_with_session<S>(
    storage: &S,
    agent_id: &AgentInstanceId,
    file_path: &str,
    new_content: &str,
    session_id: Option<EditSessionId>,
) -> Result<SnapshotId>
where
    S: SnapshotStore + DeltaStore + FileNodeStore + PartitionStore,
{
    apply_agent_edit_full(storage, agent_id, file_path, new_content, session_id, crate::engine::diff::DEFAULT_FULL_SNAPSHOT_THRESHOLD)
}

/// Agent edit with explicit full-snapshot threshold and optional session.
///
/// `threshold` is the byte-change ratio above which the edit is stored as a
/// full-content snapshot instead of a line-level delta (see
/// `should_use_full_snapshot_content`). Use
/// `DEFAULT_FULL_SNAPSHOT_THRESHOLD` for the default behavior.
pub fn apply_agent_edit_full<S>(
    storage: &S,
    agent_id: &AgentInstanceId,
    file_path: &str,
    new_content: &str,
    session_id: Option<EditSessionId>,
    threshold: f64,
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

    // Read old content — prefer inline full-content snapshot over delta chain
    let old_content = match &current_snapshot.content {
        Some(crate::core::snapshot::SnapshotContent::FileContent(bytes)) => {
            String::from_utf8_lossy(bytes).to_string()
        }
        _ => {
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
            apply_deltas(&content_str, &deltas)
                .map_err(|e| LayertwineError::Engine(e.to_string()))?
        }
    };

    // Calculate diff
    let line_diff = diff_to_line_diff(&old_content, new_content);
    if line_diff.is_empty() {
        return Ok(partition.current_snapshot);
    }

    // Check if this edit should bypass the delta chain and store full content.
    if should_use_full_snapshot_content(old_content.as_bytes(), new_content.as_bytes(), threshold) {
        let file_node = FileNode::new(PathBuf::from(file_path), new_content.as_bytes());
        let snapshot = Snapshot::new_with_content(
            file_node.clone(),
            SnapshotContent::FileContent(new_content.as_bytes().to_vec()),
            format!("agent://{}/{}", agent_id, file_path),
            PartitionType::Agent(agent_id.clone()).name(),
            vec![current_snapshot.id],
            vec![],
        );
        storage
            .store_file_node(&file_node, new_content.as_bytes())
            .map_err(LayertwineError::Storage)?;
        storage
            .store_snapshot(&snapshot, new_content.as_bytes())
            .map_err(LayertwineError::Storage)?;
        storage
            .update_pointer(&pid, &snapshot.id)
            .map_err(LayertwineError::Storage)?;
        return Ok(snapshot.id);
    }

    // Create Delta
    let file_node = FileNode::new(PathBuf::from(file_path), old_content.as_bytes());
    let delta = Delta::new_with_session(
        file_node.clone(),
        line_diff,
        SourceType::Agent(agent_id.clone()),
        session_id,
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

/// Delete a file on an agent partition (explicit deletion semantics).
///
/// Records a full-delete delta (old content → empty) and marks the new
/// snapshot with `SnapshotContent::Deleted`, so projection/restore layers
/// can tell a removed file apart from a cleared (empty) one. Reconstructing
/// the snapshot yields an empty string either way.
pub fn apply_agent_delete<S>(
    storage: &S,
    agent_id: &AgentInstanceId,
    file_path: &str,
) -> Result<SnapshotId>
where
    S: SnapshotStore + DeltaStore + FileNodeStore + PartitionStore,
{
    apply_agent_delete_with_session(storage, agent_id, file_path, None)
}

/// Delete a file with optional edit session grouping.
pub fn apply_agent_delete_with_session<S>(
    storage: &S,
    agent_id: &AgentInstanceId,
    file_path: &str,
    session_id: Option<EditSessionId>,
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

    // Read old content — prefer inline full-content snapshot over delta chain
    let old_content = match &current_snapshot.content {
        Some(SnapshotContent::FileContent(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => {
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
            apply_deltas(&content_str, &deltas)
                .map_err(|e| LayertwineError::Engine(e.to_string()))?
        }
    };

    // Full-file deletion diff: old content -> empty. An already-empty file
    // yields an empty diff, but the Deleted content marker still records
    // the deletion.
    let line_diff = diff_to_line_diff(&old_content, "");

    // Create Delta
    let file_node = FileNode::new(PathBuf::from(file_path), old_content.as_bytes());
    let delta = Delta::new_with_session(
        file_node.clone(),
        line_diff,
        SourceType::Agent(agent_id.clone()),
        session_id,
    );
    storage
        .store_file_node(&file_node, old_content.as_bytes())
        .map_err(LayertwineError::Storage)?;
    storage
        .store_delta(&delta)
        .map_err(LayertwineError::Storage)?;

    // Creating a New Snapshot carrying the explicit deletion marker. The
    // parent's delta chain is kept; only the content marker and the id are
    // recomputed.
    let mut new_snapshot = Snapshot::from_parent(
        &current_snapshot,
        delta.id,
        PartitionType::Agent(agent_id.clone()).name(),
    );
    new_snapshot.content = Some(SnapshotContent::Deleted);
    new_snapshot.id = new_snapshot.compute_id();
    storage
        .store_snapshot(&new_snapshot, b"")
        .map_err(LayertwineError::Storage)?;

    // Updating the partition pointer
    storage
        .update_pointer(&pid, &new_snapshot.id)
        .map_err(LayertwineError::Storage)?;

    Ok(new_snapshot.id)
}
///
/// Path of the seed snapshot; excluded from per-path merges.
const SEED_PATH: &str = ".wf-checkpoint-seed";

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
    history: &[crate::core::types::SnapshotId],
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
        if path == SEED_PATH {
            continue;
        }
        map.insert(path, snapshot);
    }
    Ok(map)
}

/// Raw bytes of a snapshot when available without lossy text conversion.
/// Returns `None` for delta-chain snapshots (caller falls back to text
/// reconstruction) and `Some(vec![])` for the deletion marker.
fn snapshot_raw_bytes(snapshot: &Snapshot) -> Option<Vec<u8>> {
    match &snapshot.content {
        Some(SnapshotContent::FileContent(bytes)) => Some(bytes.clone()),
        Some(SnapshotContent::Deleted) => Some(Vec::new()),
        _ => None,
    }
}

/// Whether a snapshot carries non-UTF8 file bytes (binary payload).
fn snapshot_is_binary(snapshot: &Snapshot) -> bool {
    match &snapshot.content {
        Some(SnapshotContent::FileContent(bytes)) => std::str::from_utf8(bytes).is_err(),
        _ => false,
    }
}

/// Corresponds to `move_agent_to_approval` in the architecture documentation.
///
/// Advances the approval partition per file path instead of merging only the
/// agent partition's current snapshot: every path in the agent partition's
/// latest-per-path set is three-way merged (base = approval baseline, ours =
/// approval head for that path, theirs = agent head for that path) and
/// appended sequentially. Binary payloads and deletions bypass the text
/// merge and are carried over verbatim with an explicit marker.
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

    if approval_partition.history.is_empty() {
        return Err(LayertwineError::StateMachine(
            "approval partition has empty history".into(),
        ));
    }

    let agent_latest = latest_per_path(storage, &agent_partition.history)?;
    if agent_latest.is_empty() {
        return Ok(approval_partition.current_snapshot);
    }
    let mut approval_latest = latest_per_path(storage, &approval_partition.history)?;

    // Merge baseline: the approval partition's first history entry (common
    // ancestor). Per-path base is its text when it applies to the same path,
    // otherwise empty (path created after seeding).
    let (baseline_path, baseline_text) = match approval_partition.history.first() {
        Some(baseline_id) => {
            let baseline_snapshot = storage
                .get_snapshot(baseline_id)
                .map_err(LayertwineError::Storage)?;
            let base_path = snapshot_path(storage, &baseline_snapshot).unwrap_or_default();
            let base_text =
                crate::layered::transition::reconstruct_text(storage, &baseline_snapshot)?
                    .unwrap_or_default();
            (base_path, base_text)
        }
        None => (String::new(), String::new()),
    };

    let mut head_id = approval_partition.current_snapshot;
    for (path, agent_snapshot) in &agent_latest {
        let approval_snapshot_opt = approval_latest.get(path);

        let agent_deleted = agent_snapshot.is_deleted();
        let agent_binary = snapshot_is_binary(agent_snapshot);
        let approval_binary = approval_snapshot_opt
            .map(snapshot_is_binary)
            .unwrap_or(false);
        let approval_deleted = approval_snapshot_opt
            .map(|s| s.is_deleted())
            .unwrap_or(false);

        // Idempotency across partitions: approval snapshots are merge copies
        // with different IDs than agent snapshots even for identical content,
        // so compare state (deletion marker / bytes / text) instead of IDs.
        // A redundant move (e.g. the second call inside `approve_changes`)
        // must be a no-op.
        if agent_deleted && approval_deleted {
            continue;
        }
        if !agent_deleted
            && !approval_deleted
            && !agent_binary
            && !approval_binary
            && approval_snapshot_opt.is_some()
        {
            let approval_text = crate::layered::transition::reconstruct_text(
                storage,
                approval_snapshot_opt.expect("checked some"),
            )?
            .unwrap_or_default();
            let agent_text =
                crate::layered::transition::reconstruct_text(storage, agent_snapshot)?
                    .unwrap_or_default();
            if approval_text == agent_text {
                continue;
            }
        }
        if (agent_binary || approval_binary)
            && !agent_deleted
            && !approval_deleted
            && approval_snapshot_opt.is_some()
        {
            let approval_bytes = snapshot_raw_bytes(
                approval_snapshot_opt.expect("checked some"),
            )
            .unwrap_or_default();
            let agent_bytes = snapshot_raw_bytes(agent_snapshot).unwrap_or_default();
            if approval_bytes == agent_bytes {
                continue;
            }
        }

        // Binary payloads and deletions bypass the text merge: carry the
        // agent state over verbatim so arbitrary byte sequences survive.
        if agent_deleted || agent_binary || approval_binary {
            let file_node = agent_snapshot.file.clone();
            let content = if agent_deleted {
                SnapshotContent::Deleted
            } else {
                SnapshotContent::FileContent(
                    snapshot_raw_bytes(agent_snapshot).unwrap_or_default(),
                )
            };
            let head_snapshot = storage
                .get_snapshot(&head_id)
                .map_err(LayertwineError::Storage)?;
            let new_snapshot = Snapshot::merge(
                vec![&head_snapshot, agent_snapshot],
                agent_snapshot.deltas.last().copied().unwrap_or_else(|| {
                    let file = file_node.clone();
                    let delta = Delta::new(
                        file,
                        crate::core::types::LineDiff::new(vec![]),
                        SourceType::Agent(agent_id.clone()),
                    );
                    let id = delta.id;
                    let _ = storage.store_delta(&delta);
                    id
                }),
                PartitionType::Approval(agent_id.clone()).name(),
                false,
            )?;
            let mut new_snapshot = new_snapshot;
            new_snapshot.content = Some(content);
            new_snapshot.id = new_snapshot.compute_id();
            let bytes = snapshot_raw_bytes(&new_snapshot).unwrap_or_default();
            storage
                .store_file_node(&file_node, &bytes)
                .map_err(LayertwineError::Storage)?;
            storage
                .store_snapshot(&new_snapshot, &bytes)
                .map_err(LayertwineError::Storage)?;
            storage
                .update_pointer(&approval_pid, &new_snapshot.id)
                .map_err(LayertwineError::Storage)?;
            head_id = new_snapshot.id;
            approval_latest.insert(path.clone(), new_snapshot);
            continue;
        }

        let head_snapshot = storage
            .get_snapshot(&head_id)
            .map_err(LayertwineError::Storage)?;
        let approval_text = match approval_snapshot_opt {
            Some(snap) => {
                crate::layered::transition::reconstruct_text(storage, snap)?.unwrap_or_default()
            }
            None => String::new(),
        };
        let agent_text =
            crate::layered::transition::reconstruct_text(storage, agent_snapshot)?
                .unwrap_or_default();

        // Per-path three-way base: the approval baseline text when it
        // applies to this path, otherwise empty (path created after seeding).
        let base_text = if baseline_path == *path {
            baseline_text.clone()
        } else {
            String::new()
        };
        let (merged_text, conflicts) =
            crate::engine::merge::merge_texts(&base_text, &approval_text, &agent_text);
        let has_conflicts = !conflicts.is_empty();

        // The partition history is a single linear transform chain shared by
        // all paths: the new delta must transform the actual head text into
        // the merged text, not the per-path approval text.
        let head_text =
            crate::layered::transition::reconstruct_text(storage, &head_snapshot)?
                .unwrap_or_default();
        let merge_diff = diff_to_line_diff(&head_text, &merged_text);
        if merge_diff.is_empty() {
            continue;
        }

        // The merge delta must carry the current path: `agent_snapshot.file`
        // is the parent's file (stale for delta-chain snapshots), so build
        // the node explicitly from the loop key.
        let merge_file = FileNode::new(std::path::PathBuf::from(path), head_text.as_bytes());
        let merge_delta = Delta::new(merge_file, merge_diff, SourceType::Agent(agent_id.clone()));
        storage
            .store_delta(&merge_delta)
            .map_err(LayertwineError::Storage)?;

        let new_snapshot = Snapshot::merge(
            vec![&head_snapshot, agent_snapshot],
            merge_delta.id,
            PartitionType::Approval(agent_id.clone()).name(),
            has_conflicts,
        )?;
        storage
            .store_snapshot(&new_snapshot, b"")
            .map_err(LayertwineError::Storage)?;

        storage
            .update_pointer(&approval_pid, &new_snapshot.id)
            .map_err(LayertwineError::Storage)?;
        head_id = new_snapshot.id;
        let stored = storage
            .get_snapshot(&head_id)
            .map_err(LayertwineError::Storage)?;
        approval_latest.insert(path.clone(), stored);
    }

    Ok(head_id)
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
    fn test_apply_agent_delete() {
        let storage = setup_storage();
        let agent_id = AgentInstanceId("agent-delete".into());
        let initial_id =
            create_initial_snapshot(&storage, "base\n", SourceType::Agent("test-agent".into()));
        ensure_agent_partition(&storage, &agent_id, initial_id).unwrap();

        let edited_id =
            apply_agent_edit(&storage, &agent_id, "test.txt", "base\nmodified\n").unwrap();
        let deleted_id = apply_agent_delete(&storage, &agent_id, "test.txt").unwrap();
        assert_ne!(deleted_id, edited_id);
        assert_ne!(deleted_id, initial_id);

        // The partition head snapshot carries the explicit deletion marker
        // and reconstructs to `None` (deleted, not cleared).
        let partition = storage
            .get_partition(&agent_partition_id(&agent_id))
            .unwrap();
        let head = storage.get_snapshot(&partition.current_snapshot).unwrap();
        assert!(head.is_deleted());

        let text = crate::layered::transition::reconstruct_text(&storage, &head).unwrap();
        assert!(text.is_none());
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
            redo_stack: Vec::new(),
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
