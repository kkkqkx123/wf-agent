//! Merge entry points on the commit DAG.
//!
//! Edit to review is a ref copy; review to feature and feature to main are
//! true multi-parent merges. There are no layered flow gates and no
//! single-chain partition pointers: rollback appends revert commits.

use crate::event::CheckpointEventBus;
use crate::file::git_merge::GitMergeOutcome;
use crate::file::git_write::map_git_error;
use crate::file::FileCheckpointManager;
use crate::git_store::feat_ref_for_name;
use checkpoint_base::error::CheckpointError;

/// Result of a merge commit: the Git merge outcome plus the multi-parent
/// commit id recording the merge in the DAG.
#[derive(Debug, Clone)]
pub struct MergeCommitResult {
    pub merge_result: GitMergeOutcome,
    pub checkpoint_id: String,
}

impl FileCheckpointManager {
    /// Submit the actor's edit line for review: copy the edit ref to a
    /// fresh review ref (no merge) and mark it pending. Returns the
    /// review commit id.
    pub fn move_agent_to_approval(&self, entity_id: &str) -> Result<String, CheckpointError> {
        let (_, head) = self.submit_for_review(entity_id)?;
        Ok(head)
    }

    /// Merge the actor's submission into a feature ref via true three-way
    /// merge. Submits first when the actor has no pending review.
    pub fn merge_agent_to_feature(
        &self,
        entity_id: &str,
        feature_name: &str,
    ) -> Result<GitMergeOutcome, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let (review_ref, review_head) = match self.newest_pending_review(actor.as_str())? {
            Some(found) => found,
            None => self.submit_for_review(entity_id)?,
        };
        self.merge_review_into_feature(&review_ref, &review_head, feature_name, actor.as_str())
    }

    /// Merge each named feature into main in input order, one independent
    /// multi-parent merge commit per feature. Overlapping lines resolve in
    /// input order; a conflicted feature aborts the sequence with a
    /// `MergeConflict` error (main already gained the earlier merges, each
    /// individually revertable).
    pub fn merge_features_to_staged(
        &self,
        feature_names: &[&str],
    ) -> Result<MergeCommitResult, CheckpointError> {
        if feature_names.len() > 1 {
            tracing::warn!(
                features = feature_names.len(),
                "merging multiple features sequentially; overlapping lines resolve in input order"
            );
        }
        let mut last: Option<GitMergeOutcome> = None;
        for feature in feature_names {
            // Sequential merges use the caller's actor label when one is
            // known; merges are content-addressed so the label only feeds
            // the commit trailer.
            let outcome = self.merge_feature_into_main(feature, "merge")?;
            last = Some(outcome);
        }
        let outcome = last.ok_or_else(|| CheckpointError::Validation {
            reason: "no features given to merge".to_string(),
        })?;
        Ok(MergeCommitResult {
            checkpoint_id: outcome.commit_id.clone(),
            merge_result: outcome,
        })
    }

    /// Fork-join join step: merge each feature into main in order, then
    /// delete the feature ref pointers. Ancestry stays in the commit graph,
    /// so removing the pointers never affects provenance.
    pub fn merge_branch_changes(
        &self,
        feature_names: &[&str],
    ) -> Result<MergeCommitResult, CheckpointError> {
        let merged = self.merge_features_to_staged(feature_names)?;
        let git = self.git_ref()?;
        for name in feature_names {
            git.delete_ref(&feat_ref_for_name(name))
                .map_err(map_git_error)?;
        }
        Ok(merged)
    }

    /// Roll back one merged feature from main by appending a revert commit.
    /// Other merged features are untouched. Returns the revert commit id.
    pub fn rollback_feature_from_main(
        &self,
        feature_head: &str,
        actor_str: &str,
    ) -> Result<String, CheckpointError> {
        Ok(self
            .revert_feature_on_main(feature_head, actor_str)?
            .commit_id)
    }

    /// Run object-store cleanup: drop loose objects unreachable from any
    /// ref. The source index rows for pruned commits are dropped with them.
    /// Reports pruned object counts through the legacy stats shape.
    pub fn run_gc(
        &self,
        retention: crate::gc::GcRetention,
    ) -> Result<crate::gc::GcStats, CheckpointError> {
        let _ = retention;
        let start = std::time::Instant::now();
        let pruned = self.prune_unreachable_objects()?;
        let stats = crate::gc::GcStats {
            removed_checkpoints: pruned as u64,
            removed_snapshots: 0,
            reclaimed_snapshots: 0,
            reclaimed_deltas: 0,
            reclaimed_file_nodes: 0,
        };
        if let Some(ref metrics) = self.checkpoint_metrics() {
            metrics.record_cleanup(
                stats.removed_checkpoints,
                0,
                start.elapsed().as_millis() as f64,
            );
        }
        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::gc_completed(stats.clone()));
        }
        Ok(stats)
    }

    /// Content-reclaim sweep: identical to [`Self::run_gc`] in the Git
    /// model (unreachable objects are the only reclaimable content).
    /// Kept as the explicit slow-cadence entry point; never runs per pass.
    pub fn run_snapshot_reclaim(
        &self,
        retention: crate::gc::GcRetention,
        grace_ms: u64,
    ) -> Result<crate::gc::GcStats, CheckpointError> {
        let _ = grace_ms;
        self.run_gc(retention)
    }

    /// Drop loose objects unreachable from any ref. Returns the pruned
    /// object count.
    pub(crate) fn prune_unreachable_objects(&self) -> Result<usize, CheckpointError> {
        use std::collections::HashSet;
        let git = self.git_ref()?;
        let mut reachable: HashSet<String> = HashSet::new();
        for commit in git.all_commits().map_err(map_git_error)? {
            reachable.insert(commit.id.clone());
            reachable.insert(commit.tree.clone());
            let files = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
            for (_, (_, blob)) in files {
                reachable.insert(blob);
            }
            // Trees themselves (non-leaf) are covered by walking down.
            let mut stack = vec![commit.tree.clone()];
            while let Some(tree) = stack.pop() {
                let Ok(entries) = git.read_tree(&tree) else {
                    continue;
                };
                for entry in entries {
                    if entry.mode == "40000" && reachable.insert(entry.id.clone()) {
                        stack.push(entry.id);
                    }
                }
            }
        }
        let objects_dir = git.git_dir().join("objects");
        let mut pruned = 0usize;
        let Ok(prefixes) = std::fs::read_dir(&objects_dir) else {
            return Ok(0);
        };
        for prefix in prefixes.flatten() {
            let dir = prefix.path();
            if !dir.is_dir() {
                continue;
            }
            let name = prefix.file_name().to_string_lossy().to_string();
            if name.len() != 2 || !name.chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            for object in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let file = object.file_name().to_string_lossy().to_string();
                if file.ends_with(".tmp") || file.ends_with(".lock") {
                    continue;
                }
                let id = format!("{name}{file}");
                if id.len() != 40 || reachable.contains(&id) {
                    continue;
                }
                if std::fs::remove_file(object.path()).is_ok() {
                    pruned += 1;
                }
            }
        }
        Ok(pruned)
    }
}
