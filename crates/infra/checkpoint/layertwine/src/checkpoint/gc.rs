use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use super::dag::ancestor_closure;
use crate::checkpoint::repo::CheckpointRepo;
use crate::core::types::{CheckpointId, DeltaId, SnapshotId};
use crate::error::{LayertwineError, Result};
use crate::storage::repository::{
    AtomicOps, CheckpointPersist, DeltaStore, EditSessionStore, FileNodeStore, PartitionStore,
    SnapshotStore,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GcStats {
    /// Checkpoint rows removed by the sweep.
    pub removed_checkpoints: u64,
    /// Snapshot references dropped with the removed checkpoints. Shared
    /// snapshot rows are content-addressed and retained, so this is a
    /// reference count, not a deleted-row or freed-bytes measure.
    pub removed_snapshots: u64,
    /// Snapshot rows physically deleted by the reclaim sweep.
    pub reclaimed_snapshots: u64,
    /// Delta rows physically deleted by the reclaim sweep.
    pub reclaimed_deltas: u64,
    /// File-node rows (including content bytes) physically deleted by the
    /// reclaim sweep.
    pub reclaimed_file_nodes: u64,
}

impl GcStats {
    pub fn new() -> Self {
        GcStats {
            removed_checkpoints: 0,
            removed_snapshots: 0,
            reclaimed_snapshots: 0,
            reclaimed_deltas: 0,
            reclaimed_file_nodes: 0,
        }
    }
}

impl Default for GcStats {
    fn default() -> Self {
        Self::new()
    }
}

/// GC retention policy: which checkpoints stay protected beyond the
/// built-in protected set (branch heads + ancestors).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GcRetention {
    /// Keep the N most recently created checkpoints protected
    /// (by `created_at`, newest first) even when no branch
    /// points at them. `0` = only the built-in protected set.
    pub keep_recent_heads: usize,
}

/// Protection rule shared by the in-memory sweep
/// ([`collect_protected_checkpoints`]) and the storage-level reclaim sweep:
/// branch heads plus ancestors, plus the most recent
/// `retention.keep_recent_heads` checkpoints (and their ancestors), so
/// freshly created checkpoints are not swept before a branch points at them.
/// The set is closed under ancestors, so sweeping everything outside it
/// never orphans a kept checkpoint.
fn protected_from_parts(
    heads: Vec<CheckpointId>,
    mut recent: Vec<(i64, CheckpointId)>,
    retention: GcRetention,
    parents_of: &HashMap<CheckpointId, Vec<CheckpointId>>,
) -> HashSet<CheckpointId> {
    let mut protected = ancestor_closure(heads, parents_of);
    if retention.keep_recent_heads > 0 {
        recent.sort_by_key(|(created_at, _)| Reverse(*created_at));
        let seeds = recent
            .into_iter()
            .take(retention.keep_recent_heads)
            .map(|(_, id)| id);
        protected.extend(ancestor_closure(seeds, parents_of));
    }
    protected
}

/// Collect all protected checkpoints that must never be removed.
///
/// Protected set includes:
/// 1. All branch head checkpoints and their ancestors (via BFS over all parents)
/// 2. The most recent `retention.keep_recent_heads` checkpoints (and
///    their ancestors), so freshly created checkpoints are not swept
///    before a branch points at them
///
/// The protected set is closed under ancestors, so sweeping every
/// checkpoint outside of it never orphans a kept checkpoint: a kept
/// checkpoint's parents are always kept as well.
pub fn collect_protected_checkpoints(
    repo: &CheckpointRepo,
    retention: GcRetention,
) -> HashSet<CheckpointId> {
    let heads: Vec<CheckpointId> = repo
        .list_branches()
        .iter()
        .map(|branch| branch.head)
        .collect();
    let mut parents_of: HashMap<CheckpointId, Vec<CheckpointId>> = HashMap::new();
    let mut recent: Vec<(i64, CheckpointId)> = Vec::new();
    for id in repo.dag().all_nodes() {
        if let Ok(cp) = repo.get_checkpoint(&id) {
            recent.push((cp.created_at, id));
            parents_of.insert(id, cp.parents.clone());
        }
    }
    protected_from_parts(heads, recent, retention, &parents_of)
}

/// Run garbage collection on the file-history checkpoint repository.
///
/// Mark-sweep algorithm:
/// 1. Collect protected checkpoints (branch heads + ancestors
///    + the most recent `retention.keep_recent_heads` checkpoints)
/// 2. Remove every checkpoint outside the protected set
///
/// Only checkpoint rows are reclaimed. Snapshots, deltas and file nodes
/// are content-addressed and immutable, so `removed_snapshots` counts the
/// snapshot references dropped with the removed checkpoints. Shared rows are
/// intentionally retained: use `referenced_snapshot_ids` to compute the live
/// set before any physical snapshot reclamation. This collector is distinct
/// from the execution-state cleanup policy, which selects candidates by age
/// and count before applying chain protection.
pub fn run_gc(repo: &mut CheckpointRepo, retention: GcRetention) -> Result<GcStats> {
    let protected = collect_protected_checkpoints(repo, retention);
    let all_checkpoints = repo.dag().all_nodes();
    let mut stats = GcStats::new();

    for cp_id in &all_checkpoints {
        if protected.contains(cp_id) {
            continue;
        }

        if let Ok(cp) = repo.get_checkpoint(cp_id) {
            stats.removed_snapshots += cp.baseline_snapshots.len() as u64;
            stats.removed_checkpoints += 1;
        }

        let _ = repo.remove_checkpoint(cp_id);
    }

    Ok(stats)
}

/// Run garbage collection with the default retention policy (no extra
/// recent-head protection).
pub fn collect_garbage(repo: &mut CheckpointRepo) -> Result<GcStats> {
    run_gc(repo, GcRetention::default())
}

/// Live snapshot references held by the protected set. Physical snapshot
/// reclamation must only remove ids outside this set so shared rows stay
/// intact.
pub fn referenced_snapshot_ids(
    repo: &CheckpointRepo,
    retention: GcRetention,
) -> HashSet<crate::core::types::SnapshotId> {
    let protected = collect_protected_checkpoints(repo, retention);
    let mut live = HashSet::new();
    for id in protected {
        if let Ok(cp) = repo.get_checkpoint(&id) {
            for snap in &cp.baseline_snapshots {
                live.insert(*snap);
            }
        }
    }
    live
}

/// Physically reclaim snapshot, delta, and file-node rows unreachable from
/// the live set. The live set spans protected checkpoints (same rule as
/// [`collect_protected_checkpoints`], computed from storage lists), live
/// partition currents and histories, and live session snapshots and deltas.
/// Partition histories pin their snapshots indefinitely: reclaim frees
/// checkpoint-exclusive rows (orphans, post-reset leftovers), not working
/// state.
///
/// Safety:
/// - Newborn snapshot rows (younger than `grace_ms` before `now_ms`) are
///   skipped: a concurrent creation may not have landed its checkpoint row
///   yet. Deltas and file nodes die by membership only, never by age.
/// - Marking is read-only; every delete lands in one atomic batch, so a
///   mid-sweep failure rolls back and reruns stay idempotent.
/// - Corrupt references are tolerated conservatively: a snapshot, delta, or
///   file row that cannot be loaded is skipped with a warning and never
///   deleted — the sweep prefers missing a reclaim over a wrong delete, and
///   the row stays eligible for a later run after the reference is repaired.
///
/// Cost: the sweep loads every live snapshot and live delta row once. It
/// runs on an explicit slow cadence (see
/// `FileCheckpointManager::run_snapshot_reclaim`), not on every GC pass.
pub fn reclaim_unreferenced_content<S>(
    storage: &S,
    retention: GcRetention,
    now_ms: i64,
    grace_ms: u64,
) -> Result<GcStats>
where
    S: SnapshotStore
        + DeltaStore
        + FileNodeStore
        + PartitionStore
        + CheckpointPersist
        + EditSessionStore
        + AtomicOps,
{
    let checkpoints = storage.list_checkpoints()?;
    let mut parents_of: HashMap<CheckpointId, Vec<CheckpointId>> = HashMap::new();
    let mut recent: Vec<(i64, CheckpointId)> = Vec::new();
    for cp in &checkpoints {
        parents_of.insert(cp.id, cp.parents.clone());
        recent.push((cp.created_at, cp.id));
    }
    let heads: Vec<CheckpointId> = storage
        .list_branches()?
        .into_iter()
        .map(|branch| branch.head)
        .collect();
    let protected = protected_from_parts(heads, recent, retention, &parents_of);

    // Live snapshots: protected baselines, partition currents + histories,
    // session snapshots.
    let mut live_snapshots: HashSet<SnapshotId> = HashSet::new();
    for cp in &checkpoints {
        if protected.contains(&cp.id) {
            live_snapshots.extend(cp.baseline_snapshots.iter().copied());
        }
    }
    for partition in storage.list_partitions()? {
        live_snapshots.insert(partition.current_snapshot);
        live_snapshots.extend(partition.history.iter().copied());
    }
    for session in storage.list_sessions()? {
        live_snapshots.extend(storage.get_session_snapshots(&session.id)?);
    }

    // Candidates: baselines of swept checkpoints that no live root needs.
    // Missing rows are already gone; newborn rows may belong to an
    // in-flight creation whose checkpoint row has not landed yet.
    let grace = grace_ms.min(i64::MAX as u64) as i64;
    let horizon = now_ms.saturating_sub(grace);
    let mut dead: HashMap<SnapshotId, crate::core::snapshot::Snapshot> = HashMap::new();
    for cp in &checkpoints {
        if protected.contains(&cp.id) {
            continue;
        }
        for snap_id in &cp.baseline_snapshots {
            if live_snapshots.contains(snap_id) || dead.contains_key(snap_id) {
                continue;
            }
            let Ok(snapshot) = storage.get_snapshot(snap_id) else {
                tracing::warn!(snapshot = %snap_id, "reclaim: missing snapshot row skipped");
                continue;
            };
            if snapshot.created_at >= horizon {
                continue;
            }
            dead.insert(*snap_id, snapshot);
        }
    }

    // Live deltas and files: every live snapshot's chain plus session
    // deltas. Deltas shared with a live line stay even when a dead
    // snapshot lists them.
    let mut live_deltas: HashSet<DeltaId> = HashSet::new();
    let mut live_files: HashSet<(String, [u8; 32])> = HashSet::new();
    for snap_id in &live_snapshots {
        let Ok(snapshot) = storage.get_snapshot(snap_id) else {
            tracing::warn!(snapshot = %snap_id, "reclaim: live snapshot row missing, skipped");
            continue;
        };
        live_files.insert((
            snapshot.file.path_str().to_string(),
            snapshot.file.base_hash,
        ));
        live_deltas.extend(snapshot.deltas.iter().copied());
    }
    for session in storage.list_sessions()? {
        live_deltas.extend(storage.get_session_deltas(&session.id)?);
    }
    for delta_id in live_deltas.clone() {
        let Ok(delta) = storage.get_delta(&delta_id) else {
            tracing::warn!(delta = %delta_id, "reclaim: live delta row missing, skipped");
            continue;
        };
        live_files.insert((delta.file.path_str().to_string(), delta.file.base_hash));
    }

    // Dead deltas and files drawn from the dead snapshots, minus anything
    // still referenced by the live set.
    let mut dead_deltas: HashSet<DeltaId> = HashSet::new();
    let mut dead_files: HashSet<(String, [u8; 32])> = HashSet::new();
    for snapshot in dead.values() {
        for delta_id in &snapshot.deltas {
            if !live_deltas.contains(delta_id) {
                dead_deltas.insert(*delta_id);
            }
        }
    }
    for delta_id in &dead_deltas {
        let Ok(delta) = storage.get_delta(delta_id) else {
            tracing::warn!(delta = %delta_id, "reclaim: dead delta row missing, skipped");
            continue;
        };
        dead_files.insert((delta.file.path_str().to_string(), delta.file.base_hash));
    }
    for snapshot in dead.values() {
        dead_files.insert((
            snapshot.file.path_str().to_string(),
            snapshot.file.base_hash,
        ));
    }
    dead_files.retain(|key| !live_files.contains(key));

    // Leaf-to-root deletion order inside one atomic batch: deltas, then
    // file nodes (content bytes), then snapshot rows.
    let mut stats = GcStats::new();
    let dead_snapshot_ids: Vec<SnapshotId> = dead.keys().copied().collect();
    let dead_delta_ids: Vec<DeltaId> = dead_deltas.into_iter().collect();
    let dead_file_keys: Vec<(String, [u8; 32])> = dead_files.into_iter().collect();
    storage
        .with_atomic(|storage| {
            for id in &dead_delta_ids {
                if storage.delete_delta(id)? {
                    stats.reclaimed_deltas += 1;
                }
            }
            for (path, hash) in &dead_file_keys {
                if storage.delete_file_node(path, hash)? {
                    stats.reclaimed_file_nodes += 1;
                }
            }
            for id in &dead_snapshot_ids {
                if storage.delete_snapshot(id)? {
                    stats.reclaimed_snapshots += 1;
                }
            }
            Ok(())
        })
        .map_err(LayertwineError::Storage)?;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::branch::Branch;
    use crate::checkpoint::repo::CheckpointRepo;
    use crate::checkpoint::types::{Checkpoint, CheckpointMetadata};
    use crate::core::delta::Delta;
    use crate::core::file_node::FileNode;
    use crate::core::partition::Partition;
    use crate::core::types::{ContentId, PartitionType, SourceType};
    use crate::engine::diff::diff_to_line_diff;
    use crate::storage::repository::{
        CheckpointPersist, DeltaStore, FileNodeStore, PartitionStore, SnapshotStore,
    };
    use crate::test_utils::{create_initial_snapshot, setup_storage_full};

    fn dummy_snapshot_id(n: u8) -> crate::core::types::SnapshotId {
        ContentId::from_content(&[n; 8])
    }

    fn store_commit(
        storage: &crate::storage::SqliteStorage,
        snapshots: Vec<crate::core::types::SnapshotId>,
        parents: Vec<CheckpointId>,
        author: &str,
        message: &str,
        created_at: i64,
    ) -> CheckpointId {
        let cp = Checkpoint::new_at(
            snapshots,
            parents,
            CheckpointMetadata::new(author, message),
            created_at,
        );
        storage.store_checkpoint(&cp).unwrap();
        cp.id
    }

    #[test]
    fn test_collect_protected_empty() {
        let snap = dummy_snapshot_id(1);
        let repo = CheckpointRepo::new_single(snap);
        let protected = collect_protected_checkpoints(&repo, GcRetention::default());
        assert!(!protected.is_empty(), "root checkpoint should be protected");
    }

    #[test]
    fn test_protected_includes_branch_head() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);
        let snap2 = dummy_snapshot_id(2);
        let cp_id = repo.commit_single(snap2, "second", "user").unwrap();

        let protected = collect_protected_checkpoints(&repo, GcRetention::default());
        assert!(
            protected.contains(&cp_id),
            "branch head should be protected"
        );
    }

    #[test]
    fn test_gc_no_removable() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        // Add a few commits on main
        for i in 2..=5 {
            repo.commit_single(dummy_snapshot_id(i), &format!("commit {}", i), "user")
                .unwrap();
        }

        let stats = collect_garbage(&mut repo).unwrap();
        // Nothing removable since all are on main's ancestor chain
        assert_eq!(stats.removed_checkpoints, 0);
    }

    #[test]
    fn test_gc_keeps_live_branch() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        // Create a branch, commit on it, switch back to main.
        repo.create_branch("feature").unwrap();
        repo.switch_branch("feature").unwrap();
        let snap_f1 = dummy_snapshot_id(10);
        let cp_f1 = repo
            .commit_single(snap_f1, "feature commit", "user")
            .unwrap();
        repo.switch_branch("main").unwrap();

        // The feature branch still exists so everything is protected
        let protected = collect_protected_checkpoints(&repo, GcRetention::default());
        assert!(
            protected.contains(&cp_f1),
            "feature checkpoint should be protected (branch exists)"
        );

        let stats = collect_garbage(&mut repo).unwrap();
        assert_eq!(stats.removed_checkpoints, 0);
    }

    #[test]
    fn test_gc_removes_deleted_branch() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        // Commit on main so the side branch diverges from a protected ancestor.
        let cp_main = repo
            .commit_single(dummy_snapshot_id(2), "main commit", "user")
            .unwrap();

        repo.create_branch("feature").unwrap();
        repo.switch_branch("feature").unwrap();
        let cp_f1 = repo
            .commit_single(dummy_snapshot_id(10), "feature commit", "user")
            .unwrap();
        repo.switch_branch("main").unwrap();

        // Sanity check: feature tip is protected while the branch exists.
        let protected = collect_protected_checkpoints(&repo, GcRetention::default());
        assert!(protected.contains(&cp_f1));

        // Deleting the branch detaches the side commit; GC must reclaim it
        // while keeping the main chain intact.
        repo.remove_branch("feature").unwrap();
        let stats = collect_garbage(&mut repo).unwrap();
        assert_eq!(stats.removed_checkpoints, 1);
        assert!(repo.get_checkpoint(&cp_f1).is_err());
        assert!(repo.get_checkpoint(&cp_main).is_ok());
    }

    #[test]
    fn test_gc_keeps_merged_branch_history() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);
        let snap2 = dummy_snapshot_id(2);
        let cp1 = repo.commit_single(snap2, "main commit 1", "user").unwrap();

        repo.create_branch_from("feature", cp1).unwrap();
        repo.switch_branch("feature").unwrap();
        repo.commit_single(dummy_snapshot_id(3), "feature commit 1", "user")
            .unwrap();

        // Merge feature into main: feature history becomes an ancestor of
        // the main head, so it stays protected even after branch deletion.
        repo.switch_branch("main").unwrap();
        repo.commit_single(dummy_snapshot_id(5), "main commit 2", "user")
            .unwrap();
        repo.merge_branches("feature", vec![dummy_snapshot_id(6)], "merge", "user")
            .unwrap();
        repo.remove_branch("feature").unwrap();

        let stats = collect_garbage(&mut repo).unwrap();
        assert_eq!(stats.removed_checkpoints, 0);
    }

    #[test]
    fn test_gc_real_orphan_removal() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        // Create a disconnected (orphan) checkpoint by directly adding to internal structures
        let orphan_snap = dummy_snapshot_id(99);
        let orphan_cp = crate::checkpoint::Checkpoint::new(
            vec![orphan_snap],
            vec![],
            crate::checkpoint::CheckpointMetadata::new("orphan", "orphan checkpoint"),
        );
        let orphan_id = orphan_cp.id;

        // Add to internal structures but NO edges to/from any protected checkpoint
        repo.checkpoints.insert(orphan_id, orphan_cp);
        repo.checkpoint_dag.add_node(orphan_id);

        let stats = collect_garbage(&mut repo).unwrap();
        assert!(
            stats.removed_checkpoints > 0,
            "GC should remove orphan checkpoint that has no connection to any branch"
        );
    }

    #[test]
    fn test_gc_stats_struct() {
        let stats = GcStats::new();
        assert_eq!(stats.removed_checkpoints, 0);
        assert_eq!(stats.removed_snapshots, 0);
        assert_eq!(stats.reclaimed_snapshots, 0);
        assert_eq!(stats.reclaimed_deltas, 0);
        assert_eq!(stats.reclaimed_file_nodes, 0);
    }

    #[test]
    fn reclaim_removes_exclusive_orphan_rows_but_keeps_shared() {
        let storage = setup_storage_full();
        let shared = create_initial_snapshot(&storage, "shared\n", SourceType::Manual);
        let excl = create_initial_snapshot(&storage, "exclusive\n", SourceType::Manual);

        // Live line: a branch head over the shared snapshot.
        let live = store_commit(&storage, vec![shared], vec![], "u", "live", 1_000);
        storage.store_branch(&Branch::new("main", live)).unwrap();
        // Orphan line: no branch points at it, so the sweep abandons it.
        store_commit(&storage, vec![excl], vec![], "u", "orphan", 1_000);

        let shared_snap = storage.get_snapshot(&shared).unwrap();
        let excl_snap = storage.get_snapshot(&excl).unwrap();
        let now = shared_snap
            .created_at
            .max(excl_snap.created_at)
            .saturating_add(61_000);

        let stats =
            reclaim_unreferenced_content(&storage, GcRetention::default(), now, 60_000).unwrap();
        assert_eq!(stats.reclaimed_snapshots, 1);
        assert_eq!(stats.reclaimed_deltas, 1);
        assert_eq!(stats.reclaimed_file_nodes, 1);

        assert!(!storage.snapshot_exists(&excl).unwrap());
        assert!(!storage.delta_exists(&excl_snap.deltas[0]).unwrap());
        assert!(!storage
            .file_node_exists("test.txt", &excl_snap.file.base_hash)
            .unwrap());
        assert!(storage.snapshot_exists(&shared).unwrap());
        assert!(storage.delta_exists(&shared_snap.deltas[0]).unwrap());
        assert!(storage
            .file_node_exists("test.txt", &shared_snap.file.base_hash)
            .unwrap());

        // Reruns are empty: the sweep is idempotent.
        let again =
            reclaim_unreferenced_content(&storage, GcRetention::default(), now, 60_000).unwrap();
        assert_eq!(again.reclaimed_snapshots, 0);
        assert_eq!(again.reclaimed_deltas, 0);
        assert_eq!(again.reclaimed_file_nodes, 0);
    }

    #[test]
    fn reclaim_keeps_shared_chain_rows_of_dead_snapshots() {
        let storage = setup_storage_full();
        let s1 = create_initial_snapshot(&storage, "v1\n", SourceType::Manual);

        // Dead child shares the parent's delta chain prefix.
        let parent = storage.get_snapshot(&s1).unwrap();
        let file_node = FileNode::new(std::path::PathBuf::from("test.txt"), b"v2\n");
        storage.store_file_node(&file_node, b"v2\n").unwrap();
        let delta = Delta::new(
            file_node,
            diff_to_line_diff("v1\n", "v2\n"),
            SourceType::Manual,
        );
        storage.store_delta(&delta).unwrap();
        let s2 = crate::core::snapshot::Snapshot::from_parent(
            &parent,
            delta.id,
            PartitionType::Integrated("f".to_string()).name(),
        );
        let s2_id = s2.id;
        storage.store_snapshot(&s2, b"").unwrap();

        let live = store_commit(&storage, vec![s1], vec![], "u", "live", 1_000);
        storage.store_branch(&Branch::new("main", live)).unwrap();
        store_commit(&storage, vec![s2_id], vec![], "u", "orphan", 1_000);

        let s2_row = storage.get_snapshot(&s2_id).unwrap();
        let now = s2_row.created_at.saturating_add(61_000);
        let stats =
            reclaim_unreferenced_content(&storage, GcRetention::default(), now, 60_000).unwrap();

        // The dead snapshot row, its exclusive delta, and its exclusive
        // file node go; the shared chain prefix stays.
        assert_eq!(stats.reclaimed_snapshots, 1);
        assert_eq!(stats.reclaimed_deltas, 1);
        assert_eq!(stats.reclaimed_file_nodes, 1);
        assert!(!storage.snapshot_exists(&s2_id).unwrap());
        assert!(!storage.delta_exists(&delta.id).unwrap());
        assert!(storage.snapshot_exists(&s1).unwrap());
        for delta_id in &parent.deltas {
            assert!(
                storage.delta_exists(delta_id).unwrap(),
                "shared chain delta must survive"
            );
        }
        assert!(storage
            .file_node_exists("test.txt", &parent.file.base_hash)
            .unwrap());
    }

    #[test]
    fn reclaim_keeps_partition_pinned_snapshots() {
        let storage = setup_storage_full();
        let pinned = create_initial_snapshot(&storage, "pinned\n", SourceType::Manual);
        store_commit(&storage, vec![pinned], vec![], "u", "orphan", 1_000);
        let partition = Partition::new(
            "integrated/pf".to_string(),
            PartitionType::Integrated("pf".to_string()),
            pinned,
        );
        storage.create_partition(&partition).unwrap();

        let snap = storage.get_snapshot(&pinned).unwrap();
        let stats = reclaim_unreferenced_content(
            &storage,
            GcRetention::default(),
            snap.created_at.saturating_add(61_000),
            60_000,
        )
        .unwrap();
        assert_eq!(stats.reclaimed_snapshots, 0);
        assert_eq!(stats.reclaimed_deltas, 0);
        assert_eq!(stats.reclaimed_file_nodes, 0);
        assert!(storage.snapshot_exists(&pinned).unwrap());
    }

    #[test]
    fn reclaim_grace_protects_newborn_orphan_rows() {
        let storage = setup_storage_full();
        let young = create_initial_snapshot(&storage, "young\n", SourceType::Manual);
        store_commit(&storage, vec![young], vec![], "u", "orphan", 1_000);

        // Inside the grace horizon the row may belong to an in-flight
        // creation whose checkpoint row has not landed yet: skip it.
        let snap = storage.get_snapshot(&young).unwrap();
        let stats = reclaim_unreferenced_content(
            &storage,
            GcRetention::default(),
            snap.created_at.saturating_add(1_000),
            60_000,
        )
        .unwrap();
        assert_eq!(stats.reclaimed_snapshots, 0);
        assert!(storage.snapshot_exists(&young).unwrap());

        // Past the horizon the same row is reclaimed.
        let stats = reclaim_unreferenced_content(
            &storage,
            GcRetention::default(),
            snap.created_at.saturating_add(61_000),
            60_000,
        )
        .unwrap();
        assert_eq!(stats.reclaimed_snapshots, 1);
        assert!(!storage.snapshot_exists(&young).unwrap());
    }

    #[test]
    fn reclaim_on_empty_storage_is_empty() {
        let storage = setup_storage_full();
        let stats =
            reclaim_unreferenced_content(&storage, GcRetention::default(), 1_000_000, 60_000)
                .unwrap();
        assert_eq!(stats.reclaimed_snapshots, 0);
        assert_eq!(stats.reclaimed_deltas, 0);
        assert_eq!(stats.reclaimed_file_nodes, 0);
    }

    #[test]
    fn test_keep_recent_heads_protects_orphan() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        let orphan_snap = dummy_snapshot_id(99);
        let orphan_cp = crate::checkpoint::Checkpoint::new(
            vec![orphan_snap],
            vec![],
            crate::checkpoint::CheckpointMetadata::new("orphan", "orphan checkpoint"),
        );
        let orphan_id = orphan_cp.id;
        repo.checkpoints.insert(orphan_id, orphan_cp);
        repo.checkpoint_dag.add_node(orphan_id);

        // Pin the orphan as newest so the retention window must cover it.
        repo.get_checkpoint_mut(&orphan_id).unwrap().created_at = i64::MAX;

        let retention = GcRetention {
            keep_recent_heads: 1,
        };
        let protected = collect_protected_checkpoints(&repo, retention);
        assert!(
            protected.contains(&orphan_id),
            "recent-head retention should protect the newest orphan"
        );
        let stats = run_gc(&mut repo, retention).unwrap();
        assert_eq!(stats.removed_checkpoints, 0);
    }

    #[test]
    fn test_removed_snapshots_count() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        // Create commits with multiple snapshots
        repo.commit_single(dummy_snapshot_id(2), "commit 2", "user")
            .unwrap();

        // Create an orphan with multiple snapshots
        let orphan_cp = crate::checkpoint::Checkpoint::new(
            vec![
                dummy_snapshot_id(10),
                dummy_snapshot_id(11),
                dummy_snapshot_id(12),
            ],
            vec![],
            crate::checkpoint::CheckpointMetadata::new("orphan", "orphan"),
        );
        let orphan_id = orphan_cp.id;
        repo.checkpoints.insert(orphan_id, orphan_cp);
        repo.checkpoint_dag.add_node(orphan_id);

        let stats = collect_garbage(&mut repo).unwrap();
        assert_eq!(
            stats.removed_checkpoints, 1,
            "should remove 1 orphan checkpoint"
        );
        assert_eq!(
            stats.removed_snapshots, 3,
            "should remove 3 snapshots from the orphan"
        );
    }
}
