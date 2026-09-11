use std::collections::{HashSet, VecDeque};

use crate::checkpoint::repo::CheckpointRepo;
use crate::core::types::CheckpointId;
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GcStats {
    pub removed_checkpoints: u64,
    pub removed_snapshots: u64,
}

impl GcStats {
    pub fn new() -> Self {
        GcStats {
            removed_checkpoints: 0,
            removed_snapshots: 0,
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

/// Walk backwards from `seeds`, inserting each checkpoint and all of
/// its ancestors into `protected`.
fn protect_with_ancestors(
    repo: &CheckpointRepo,
    protected: &mut HashSet<CheckpointId>,
    seeds: Vec<CheckpointId>,
) {
    let mut queue: VecDeque<CheckpointId> = seeds.into_iter().collect();
    while let Some(id) = queue.pop_front() {
        if !protected.insert(id) {
            continue;
        }
        if let Ok(cp) = repo.get_checkpoint(&id) {
            for parent in &cp.parents {
                if !protected.contains(parent) {
                    queue.push_back(*parent);
                }
            }
        }
    }
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
    let mut protected = HashSet::new();

    // All branch heads and their ancestors (traverse ALL parents, not just first)
    let heads: Vec<CheckpointId> =
        repo.list_branches().iter().map(|branch| branch.head).collect();
    protect_with_ancestors(repo, &mut protected, heads);

    // Most recent checkpoints (and their ancestors), newest first
    if retention.keep_recent_heads > 0 {
        let mut recent: Vec<(i64, CheckpointId)> = repo
            .dag()
            .all_nodes()
            .into_iter()
            .filter_map(|id| repo.get_checkpoint(&id).ok().map(|cp| (cp.created_at, id)))
            .collect();
        recent.sort_by_key(|(created_at, _)| std::cmp::Reverse(*created_at));
        let seeds: Vec<CheckpointId> = recent
            .into_iter()
            .take(retention.keep_recent_heads)
            .map(|(_, id)| id)
            .collect();
        protect_with_ancestors(repo, &mut protected, seeds);
    }

    protected
}

/// Run garbage collection on the checkpoint repository.
///
/// Mark-sweep algorithm:
/// 1. Collect protected checkpoints (branch heads + ancestors
///    + the most recent `retention.keep_recent_heads` checkpoints)
/// 2. Remove every checkpoint outside the protected set
///
/// Only checkpoint rows are reclaimed. Snapshots, deltas and file nodes
/// are content-addressed and immutable, so `removed_snapshots` counts the
/// snapshot references dropped with the removed checkpoints.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::repo::CheckpointRepo;
    use crate::core::types::ContentId;

    fn dummy_snapshot_id(n: u8) -> crate::core::types::SnapshotId {
        ContentId::from_content(&[n; 8])
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
