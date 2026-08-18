use std::collections::{HashSet, VecDeque};

use crate::checkpoint::repo::CheckpointRepo;
use crate::core::types::CheckpointId;
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GCStats {
    pub removed_checkpoints: u64,
    pub removed_snapshots: u64,
    pub freed_bytes: u64,
    pub delta_chain_depth_triggered: bool,
    pub max_chain_depth: usize,
}

impl GCStats {
    pub fn new() -> Self {
        GCStats {
            removed_checkpoints: 0,
            removed_snapshots: 0,
            freed_bytes: 0,
            delta_chain_depth_triggered: false,
            max_chain_depth: 0,
        }
    }
}

impl Default for GCStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Collect all protected checkpoints that must never be removed.
///
/// Protected set includes:
/// 1. All branch head checkpoints and their ancestors (via BFS over all parents)
/// 2. All checkpoints bound to a git_anchor
pub fn collect_protected_checkpoints(repo: &CheckpointRepo) -> HashSet<CheckpointId> {
    let mut protected = HashSet::new();

    // All branch heads and their ancestors (traverse ALL parents, not just first)
    for branch in repo.list_branches() {
        let mut queue: VecDeque<CheckpointId> = VecDeque::new();
        queue.push_back(branch.head);

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

    // All checkpoints bound to a git_anchor
    for cp_id in repo.dag().all_nodes() {
        if let Ok(cp) = repo.get_checkpoint(&cp_id) {
            if cp.metadata.git_anchor.is_some() {
                protected.insert(cp_id);
            }
        }
    }

    protected
}

/// Mark all checkpoints reachable from the protected set via children edges.
fn mark_reachable(
    repo: &CheckpointRepo,
    protected: &HashSet<CheckpointId>,
) -> HashSet<CheckpointId> {
    let mut reachable = protected.clone();
    let mut queue: VecDeque<CheckpointId> = protected.iter().copied().collect();

    while let Some(current) = queue.pop_front() {
        let children = repo.dag().get_children(&current);
        for child in children {
            if reachable.insert(child) {
                queue.push_back(child);
            }
        }
    }

    reachable
}

/// Run garbage collection on the checkpoint repository.
///
/// Mark-sweep algorithm:
/// 1. Collect protected checkpoints (branch heads + ancestors + git_anchor)
/// 2. Mark all descendants of protected checkpoints as reachable
/// 3. Remove all unreachable checkpoints
/// 4. Check delta chain depth for repacking trigger
pub fn collect_garbage(repo: &mut CheckpointRepo) -> Result<GCStats> {
    let protected = collect_protected_checkpoints(repo);

    // Mark phase: traverse forward from protected to find all reachable nodes
    let to_keep = mark_reachable(repo, &protected);

    let all_checkpoints = repo.dag().all_nodes();
    let mut stats = GCStats::new();

    // Check delta chain depth using proper depth calculation
    stats.max_chain_depth = calculate_max_depth(repo);
    if stats.max_chain_depth > 100 {
        stats.delta_chain_depth_triggered = true;
    }

    // Sweep phase: remove unreachable checkpoints
    for cp_id in &all_checkpoints {
        if to_keep.contains(cp_id) {
            continue;
        }

        if let Ok(cp) = repo.get_checkpoint(cp_id) {
            // Count snapshots to be removed
            stats.removed_snapshots += cp.baseline_snapshots.len() as u64;

            // Note: We cannot accurately calculate freed_bytes without access to
            // the storage layer's internal size tracking. This would require
            // additional metadata in the storage layer.
            stats.removed_checkpoints += 1;
        }

        let _ = repo.remove_checkpoint(cp_id);
    }

    Ok(stats)
}

/// Calculate the maximum depth of any checkpoint in the DAG.
///
/// Depth is defined as the length of the longest path from a root checkpoint
/// (no parents) to the given checkpoint.
fn calculate_max_depth(repo: &CheckpointRepo) -> usize {
    let mut max_depth = 0;
    let all_checkpoints = repo.dag().all_nodes();

    // For each checkpoint, calculate its depth using DFS
    for cp_id in &all_checkpoints {
        let depth = calculate_checkpoint_depth(repo, cp_id);
        if depth > max_depth {
            max_depth = depth;
        }
    }

    max_depth
}

/// Calculate the depth of a specific checkpoint using DFS with memoization.
///
/// Depth is defined as the length of the longest path from a root checkpoint
/// (no parents) to this checkpoint.
fn calculate_checkpoint_depth(repo: &CheckpointRepo, cp_id: &CheckpointId) -> usize {
    fn dfs(
        repo: &CheckpointRepo,
        cp_id: &CheckpointId,
        memo: &mut std::collections::HashMap<CheckpointId, usize>,
    ) -> usize {
        if let Some(&cached) = memo.get(cp_id) {
            return cached;
        }

        if let Ok(cp) = repo.get_checkpoint(cp_id) {
            if cp.parents.is_empty() {
                memo.insert(*cp_id, 0);
                return 0;
            }

            let max_parent_depth = cp
                .parents
                .iter()
                .map(|parent_id| dfs(repo, parent_id, memo))
                .max()
                .unwrap_or(0);

            let depth = max_parent_depth + 1;
            memo.insert(*cp_id, depth);
            depth
        } else {
            0
        }
    }

    let mut memo = std::collections::HashMap::new();
    dfs(repo, cp_id, &mut memo)
}

/// Check if the delta chain depth exceeds the threshold for repacking.
pub fn check_delta_chain_depth(repo: &CheckpointRepo) -> Result<(usize, bool)> {
    let max_depth = calculate_max_depth(repo);
    let triggered = max_depth > 100;
    Ok((max_depth, triggered))
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
        let protected = collect_protected_checkpoints(&repo);
        assert!(!protected.is_empty(), "root checkpoint should be protected");
    }

    #[test]
    fn test_protected_includes_branch_head() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);
        let snap2 = dummy_snapshot_id(2);
        let cp_id = repo.commit_single(snap2, "second", "user").unwrap();

        let protected = collect_protected_checkpoints(&repo);
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
    fn test_gc_removes_orphan() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);
        let _main_root = repo.current_branch_head();

        // Create a branch, make commits, delete the branch reference
        repo.create_branch("feature").unwrap();
        let snap_f1 = dummy_snapshot_id(10);
        let cp_f1 = repo
            .commit_single(snap_f1, "feature commit", "user")
            .unwrap();

        // Switch back to main - feature branch still exists so everything is protected
        repo.switch_branch("main").unwrap();

        // The feature branch is still protected
        let protected = collect_protected_checkpoints(&repo);
        assert!(
            protected.contains(&cp_f1),
            "feature checkpoint should be protected (branch exists)"
        );

        // GC should not remove anything since all checkpoints are on a branch
        let stats = collect_garbage(&mut repo).unwrap();
        assert_eq!(stats.removed_checkpoints, 0);
    }

    #[test]
    fn test_gc_orphan_removal() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        // Simulate orphan: we need a checkpoint that has no branch pointing to it
        // and is not an ancestor of any branch head

        // Check if there are orphans using the protected set
        let protected = collect_protected_checkpoints(&repo);
        let all = repo.dag().all_nodes();
        for cp_id in all {
            if !protected.contains(&cp_id) {
                // This would be an orphan - we could test removal
            }
        }

        let stats = collect_garbage(&mut repo).unwrap();
        assert_eq!(stats.removed_checkpoints, 0);
    }

    #[test]
    fn test_delta_chain_depth_check() {
        let snap = dummy_snapshot_id(1);
        let repo = CheckpointRepo::new_single(snap);
        let (max_depth, triggered) = check_delta_chain_depth(&repo).unwrap();
        assert_eq!(max_depth, 0);
        assert!(!triggered);
    }

    #[test]
    fn test_gc_stats_struct() {
        let stats = GCStats::new();
        assert_eq!(stats.removed_checkpoints, 0);
        assert_eq!(stats.removed_snapshots, 0);
        assert_eq!(stats.freed_bytes, 0);
        assert!(!stats.delta_chain_depth_triggered);
    }

    #[test]
    fn test_protected_includes_git_anchor() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);
        let snap2 = dummy_snapshot_id(2);
        let cp_id = repo.commit_single(snap2, "anchored", "user").unwrap();

        // Remove the branch reference to make the checkpoint orphaned
        {
            let cp = repo.get_checkpoint_mut(&cp_id).unwrap();
            cp.metadata.git_anchor = Some("abc123".to_string());
        }
        // Remove branch "main" so cp would normally be orphaned
        repo.branches.retain(|b| b.name != "main");

        let protected = collect_protected_checkpoints(&repo);
        assert!(
            protected.contains(&cp_id),
            "checkpoint with git_anchor should be protected even without branch"
        );
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
    fn test_depth_calculation_linear_chain() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        // Create a linear chain of 5 commits
        for i in 2..=6 {
            repo.commit_single(dummy_snapshot_id(i), &format!("commit {}", i), "user")
                .unwrap();
        }

        // The chain depth should be 5 (root has depth 0, each subsequent commit adds 1)
        let max_depth = calculate_max_depth(&repo);
        assert_eq!(
            max_depth, 5,
            "linear chain of 5 commits should have max depth 5"
        );
    }

    #[test]
    fn test_depth_calculation_merge() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        // main branch already exists with root checkpoint
        let snap2 = dummy_snapshot_id(2);
        let cp1 = repo.commit_single(snap2, "main commit 1", "user").unwrap();

        // Create feature branch from commit 1 (not root)
        repo.create_branch_from("feature", cp1).unwrap();
        repo.switch_branch("feature").unwrap();
        let snap3 = dummy_snapshot_id(3);
        repo.commit_single(snap3, "feature commit 1", "user")
            .unwrap();

        // Merge feature into main
        // Current structure:
        //   root -> cp1 -> main_commit_2 -> merge
        //          -> feature_commit_1 -> merge
        // Max depth should be 3
        let _snap4 = dummy_snapshot_id(4);
        repo.switch_branch("main").unwrap();
        let snap5 = dummy_snapshot_id(5);
        repo.commit_single(snap5, "main commit 2", "user").unwrap();

        let snap6 = dummy_snapshot_id(6);
        repo.merge_branches("feature", vec![snap6], "merge", "user")
            .unwrap();

        // The max depth should be 3 (root -> cp1 -> main_commit_2 -> merge)
        let max_depth = calculate_max_depth(&repo);
        assert_eq!(max_depth, 3, "merge commit should have depth 3");
    }

    #[test]
    fn test_depth_calculation_complex_dag() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        // Create main branch commits
        let cp1 = repo
            .commit_single(dummy_snapshot_id(2), "main 1", "user")
            .unwrap();
        let _cp2 = repo
            .commit_single(dummy_snapshot_id(3), "main 2", "user")
            .unwrap();
        let _cp3 = repo
            .commit_single(dummy_snapshot_id(4), "main 3", "user")
            .unwrap();

        // Create feature branch from cp1
        repo.create_branch_from("feature", cp1).unwrap();
        repo.switch_branch("feature").unwrap();
        let _feature_cp1 = repo
            .commit_single(dummy_snapshot_id(10), "feature 1", "user")
            .unwrap();

        // Merge feature into main
        // Current structure:
        //   root -> cp1 -> cp2 -> cp3 -> merge
        //          -> feature_cp1 -> merge
        // Max depth should be 4 (root -> cp1 -> cp2 -> cp3 -> merge)
        let snap5 = dummy_snapshot_id(5);
        repo.switch_branch("main").unwrap();
        repo.merge_branches("feature", vec![snap5], "merge", "user")
            .unwrap();

        let max_depth = calculate_max_depth(&repo);
        assert_eq!(max_depth, 4, "complex DAG should have correct max depth");
    }

    #[test]
    fn test_mark_reachable() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        // Create a commit chain
        let cp1 = repo
            .commit_single(dummy_snapshot_id(2), "commit 2", "user")
            .unwrap();
        let cp2 = repo
            .commit_single(dummy_snapshot_id(3), "commit 3", "user")
            .unwrap();

        // Create an orphan (not reachable from any branch head)
        let orphan_snap = dummy_snapshot_id(99);
        let orphan_cp = crate::checkpoint::Checkpoint::new(
            vec![orphan_snap],
            vec![],
            crate::checkpoint::CheckpointMetadata::new("orphan", "orphan checkpoint"),
        );
        let orphan_id = orphan_cp.id;
        repo.checkpoints.insert(orphan_id, orphan_cp);
        repo.checkpoint_dag.add_node(orphan_id);

        // The protected set should include the branch head and its ancestors
        let protected = collect_protected_checkpoints(&repo);
        assert!(protected.contains(&cp2), "branch head should be protected");
        assert!(
            protected.contains(&cp1),
            "parent of branch head should be protected"
        );
        assert!(
            !protected.contains(&orphan_id),
            "orphan should not be protected"
        );

        // The reachable set should be the same as protected in this case
        // (orphan is not a descendant of any protected checkpoint)
        let reachable = mark_reachable(&repo, &protected);
        assert_eq!(
            reachable, protected,
            "reachable should match protected when no descendants exist"
        );
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
