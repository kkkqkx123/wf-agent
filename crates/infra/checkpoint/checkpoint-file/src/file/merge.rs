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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MergeCommitResult {
    pub merge_result: GitMergeOutcome,
    pub checkpoint_id: String,
}

/// Default in-flight grace for object-store cleanup: unreachable loose
/// objects younger than this are kept because they may belong to a commit
/// whose ref update has not landed yet.
pub const DEFAULT_GC_INFLIGHT_GRACE: std::time::Duration = std::time::Duration::from_secs(300);

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
        crate::branch::ensure_feature_branch_name(feature_name)?;
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
    pub fn merge_features_to_main(
        &self,
        feature_names: &[&str],
    ) -> Result<MergeCommitResult, CheckpointError> {
        for feature in feature_names {
            crate::branch::ensure_feature_branch_name(feature)?;
        }
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
        let merged = self.merge_features_to_main(feature_names)?;
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

    /// Run object-store cleanup: drop loose objects unreachable from any ref,
    /// except the newest retained commits. The source index rows for pruned
    /// commits are dropped with them.
    pub fn run_gc(
        &self,
        retention: crate::gc::GcRetention,
    ) -> Result<crate::gc::GcStats, CheckpointError> {
        self.run_gc_with_grace(retention, DEFAULT_GC_INFLIGHT_GRACE)
    }

    /// Run object-store cleanup with an explicit in-flight grace window:
    /// unreachable objects younger than `grace` are kept because they may
    /// belong to a commit whose ref update has not landed yet.
    pub fn run_gc_with_grace(
        &self,
        retention: crate::gc::GcRetention,
        grace: std::time::Duration,
    ) -> Result<crate::gc::GcStats, CheckpointError> {
        let start = std::time::Instant::now();
        let (commits, trees, blobs) = self.prune_unreachable_objects(&retention, grace)?;
        let mut stats = crate::gc::GcStats {
            removed_checkpoints: commits as u64,
            removed_snapshots: trees as u64,
            reclaimed_snapshots: blobs as u64,
            removed_edit_refs: 0,
            packed_objects: 0,
        };
        stats.removed_edit_refs = self.collect_merged_edit_refs() as u64;
        stats.packed_objects = self.pack_loose_objects().unwrap_or(0) as u64;
        self.record_last_gc();
        if let Some(ref metrics) = self.checkpoint_metrics() {
            metrics.record_cleanup(
                stats.removed_checkpoints,
                stats.reclaimed_snapshots,
                start.elapsed().as_millis() as f64,
            );
        }
        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::gc_completed(stats.clone()));
        }
        Ok(stats)
    }

    /// Record the completion time of the GC run so the storage dashboard
    /// can report it. Auxiliary: a failed write never fails the GC that
    /// already succeeded, but it is logged so a broken store stays visible.
    fn record_last_gc(&self) {
        let Ok(storage) = self.storage_ref() else {
            return;
        };
        let now = chrono::Utc::now().timestamp_millis();
        if let Err(err) = crate::storage::MetadataStore::store_metadata(
            storage,
            crate::gc::LAST_GC_KEY,
            &now.to_string(),
        ) {
            tracing::warn!(error = %err, "failed to record last GC timestamp");
        }
    }

    /// Content-reclaim sweep: identical to [`Self::run_gc`] in the Git
    /// model (unreachable objects are the only reclaimable content).
    /// Kept as the explicit slow-cadence entry point; never runs per pass.
    /// The grace window protects recently written in-flight objects.
    pub fn run_snapshot_reclaim(
        &self,
        retention: crate::gc::GcRetention,
        grace_ms: u64,
    ) -> Result<crate::gc::GcStats, CheckpointError> {
        self.run_gc_with_grace(retention, std::time::Duration::from_millis(grace_ms))
    }

    /// Drop loose objects unreachable from any ref, protecting the newest
    /// retained commits. Unreachable objects younger than `grace` are kept:
    /// they may belong to an in-flight commit whose ref update has not
    /// landed yet. Returns pruned commit, tree and blob counts.
    pub(crate) fn prune_unreachable_objects(
        &self,
        retention: &crate::gc::GcRetention,
        grace: std::time::Duration,
    ) -> Result<(usize, usize, usize), CheckpointError> {
        use std::collections::HashSet;
        let git = self.git_ref()?;
        let mut reachable: HashSet<String> = HashSet::new();
        for commit in git.all_commits().map_err(map_git_error)? {
            Self::insert_commit_closure(git, &commit.id, &commit.tree, &mut reachable);
        }
        if retention.keep_recent_heads > 0 {
            let mut orphan_commits = Vec::new();
            for id in Self::list_object_ids(git) {
                if reachable.contains(&id) {
                    continue;
                }
                if let Ok(commit) = git.read_commit(&id) {
                    orphan_commits.push((commit.committer_ts, commit.id.clone()));
                }
            }
            orphan_commits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
            for (_, id) in orphan_commits.into_iter().take(retention.keep_recent_heads) {
                Self::insert_ancestry_closure(git, &id, &mut reachable);
            }
        }
        let mut pruned_commits = 0usize;
        let mut pruned_trees = 0usize;
        let mut pruned_blobs = 0usize;
        let mut pruned_commit_ids = Vec::new();
        for id in Self::list_object_ids(git) {
            if reachable.contains(&id) {
                continue;
            }
            if Self::object_is_within_grace(git, &id, grace) {
                continue;
            }
            let is_commit = git.read_commit(&id).is_ok();
            let is_tree = !is_commit && git.read_tree(&id).is_ok();
            if Self::remove_object_file(git, &id) {
                if is_commit {
                    pruned_commits += 1;
                    pruned_commit_ids.push(id);
                } else if is_tree {
                    pruned_trees += 1;
                } else {
                    pruned_blobs += 1;
                }
            }
        }
        if !pruned_commit_ids.is_empty() {
            if let Ok(storage) = self.storage_ref() {
                for id in &pruned_commit_ids {
                    let _ = storage.delete_source_index(id);
                    let _ = storage.delete_empty_dirs(id);
                    let _ = storage.delete_oversized_files(id);
                }
            }
        }
        Ok((pruned_commits, pruned_trees, pruned_blobs))
    }

    fn insert_ancestry_closure(
        git: &crate::git_store::GitStore,
        commit_id: &str,
        reachable: &mut std::collections::HashSet<String>,
    ) {
        let mut stack = vec![commit_id.to_string()];
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(commit) = git.read_commit(&id) else {
                continue;
            };
            Self::insert_commit_closure(git, &commit.id, &commit.tree, reachable);
            for parent in commit.parents {
                stack.push(parent);
            }
        }
    }

    fn insert_commit_closure(
        git: &crate::git_store::GitStore,
        commit_id: &str,
        tree_id: &str,
        reachable: &mut std::collections::HashSet<String>,
    ) {
        reachable.insert(commit_id.to_string());
        if !reachable.insert(tree_id.to_string()) {
            return;
        }
        let Ok(files) = git.tree_to_files(tree_id) else {
            return;
        };
        for (_, (_, blob)) in files {
            reachable.insert(blob);
        }
        let mut stack = vec![tree_id.to_string()];
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

    pub(crate) fn list_object_ids(git: &crate::git_store::GitStore) -> Vec<String> {
        let objects = git.git_dir().join("objects");
        if !objects.is_dir() {
            return Vec::new();
        }
        gix_odb::loose::Store::at(objects, gix_hash::Kind::Sha1)
            .iter()
            .filter_map(|id| id.ok())
            .map(|id| id.to_hex().to_string())
            .collect()
    }

    fn remove_object_file(git: &crate::git_store::GitStore, id: &str) -> bool {
        let path = git.git_dir().join("objects").join(&id[..2]).join(&id[2..]);
        std::fs::remove_file(path).is_ok()
    }

    /// Whether a loose object was written recently enough to still be part
    /// of an in-flight commit. Objects whose age cannot be determined are
    /// treated as new so cleanup favors safety over progress.
    fn object_is_within_grace(
        git: &crate::git_store::GitStore,
        id: &str,
        grace: std::time::Duration,
    ) -> bool {
        if id.len() < 3 {
            return false;
        }
        let path = git.git_dir().join("objects").join(&id[..2]).join(&id[2..]);
        let Ok(metadata) = std::fs::metadata(&path) else {
            return true;
        };
        let Ok(modified) = metadata.modified() else {
            return true;
        };
        modified.elapsed().map(|age| age < grace).unwrap_or(true)
    }

    /// Drop edit refs whose full history is already contained in the
    /// mainline: after an approval merge the actor line has no future and
    /// keeping its ref only pins ancestors against reclamation forever.
    /// Mainline containment is judged by ancestry against `refs/wf/main`;
    /// unresolvable states keep the ref (cleanup favors safety).
    fn collect_merged_edit_refs(&self) -> usize {
        let git = match self.git_ref() {
            Ok(git) => git,
            Err(_) => return 0,
        };
        let Some(main_head) = git.read_ref(crate::git_store::REF_MAIN).ok().flatten() else {
            return 0;
        };
        let edit_refs = match git.list_refs(crate::git_store::REF_EDIT_PREFIX) {
            Ok(refs) => refs,
            Err(_) => return 0,
        };
        let mut removed = 0usize;
        for (name, head) in edit_refs {
            let merged = git
                .is_ancestor(&head, &main_head)
                .or_else(|_| -> Result<bool, crate::git_store::GitStoreError> {
                    let equal = git.read_ref(&name).ok().flatten().as_deref()
                        == Some(head.as_str())
                        && head == main_head;
                    Ok(equal)
                })
                .unwrap_or(false);
            if merged && git.delete_ref(&name).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    /// Loose-object count above which a packfile compaction is worth its
    /// cost. Below the threshold the per-object fan-out is harmless and
    /// packing would churn the store on every GC run.
    const PACK_THRESHOLD: usize = 1024;

    /// Fold loose objects into one packfile to cut inode and directory
    /// pressure on long-running stores. Best effort via the `git` binary
    /// (`pack-objects`): the object store is read back through gix either
    /// way, and any failure simply leaves the store unpacked. Only objects
    /// still present at pack time are deleted, so a concurrent writer never
    /// loses an object.
    fn pack_loose_objects(&self) -> Option<usize> {
        use std::io::Write as _;
        let git = self.git_ref().ok()?;
        let ids = Self::list_object_ids(git);
        if ids.len() < Self::PACK_THRESHOLD {
            return Some(0);
        }
        let git_dir = git.git_dir();
        let staging = tempfile::tempdir().ok()?;
        let mut child = std::process::Command::new("git")
            .arg("--git-dir")
            .arg(git_dir)
            .arg("pack-objects")
            .arg("-q")
            .arg(staging.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        {
            // Object names are fed line by line; closing stdin (end of
            // scope) signals pack-objects to finish and exit.
            let stdin = child.stdin.as_mut()?;
            for id in &ids {
                if writeln!(stdin, "{id}").is_err() {
                    return None;
                }
            }
        }
        let status = child.wait().ok()?;
        if !status.success() {
            tracing::warn!("loose-object packing failed; leaving the store unpacked");
            return None;
        }
        let pack_name = std::fs::read_dir(staging.path())
            .ok()?
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| name.ends_with(".pack"))?;
        let idx_name = pack_name.replace(".pack", ".idx");
        if !staging.path().join(&idx_name).exists() {
            return None;
        }
        let pack_dir = git_dir.join("objects/pack");
        std::fs::create_dir_all(&pack_dir).ok()?;
        std::fs::rename(staging.path().join(&pack_name), pack_dir.join(&pack_name)).ok()?;
        std::fs::rename(staging.path().join(&idx_name), pack_dir.join(&idx_name)).ok()?;
        // Drop the loose copies only after both pack files are in place.
        let mut packed = 0usize;
        for id in &ids {
            if Self::remove_object_file(git, id) {
                packed += 1;
            }
        }
        Some(packed)
    }
}

#[cfg(test)]
mod tests {
    use crate::file::FileCheckpointManager;
    use crate::gc::{GcRetention, GcStats};
    use crate::git_store::{edit_ref_for_actor, MODE_FILE, REF_MAIN, SYSTEM_COMMITTER};
    use std::collections::HashMap;
    use std::time::Duration;

    fn manager() -> FileCheckpointManager {
        FileCheckpointManager::new_in_memory().expect("in-memory manager")
    }

    fn changes(pairs: &[(&str, &[u8])]) -> HashMap<String, Option<(String, Vec<u8>)>> {
        pairs
            .iter()
            .map(|(p, b)| (p.to_string(), Some((MODE_FILE.to_string(), b.to_vec()))))
            .collect()
    }

    #[test]
    fn gc_prunes_unreachable_history_and_stays_idempotent() {
        let manager = manager();
        let actor = manager.actor_id_for("entity-1");
        manager.apply_agent_edit(&actor, "a.txt", b"v1\n").unwrap();
        manager.apply_agent_edit(&actor, "a.txt", b"v2\n").unwrap();
        manager.apply_agent_edit(&actor, "a.txt", b"v3\n").unwrap();

        // A second actor leaves its own edit line; deleting that ref makes
        // its commit, tree and blob unreachable.
        let other = manager.actor_id_for("entity-2");
        manager.apply_agent_edit(&other, "b.txt", b"gone\n").unwrap();
        let other_ref = edit_ref_for_actor(other.as_str());
        assert!(manager.git_ref().unwrap().read_ref(&other_ref).unwrap().is_some());
        manager.git_ref().unwrap().delete_ref(&other_ref).unwrap();

        let stats = manager
            .run_gc_with_grace(GcRetention::default(), Duration::ZERO)
            .unwrap();
        // The dropped edit line is reclaimed; the live line is untouched.
        assert_eq!(stats.removed_checkpoints, 1);
        assert!(stats.removed_snapshots >= 1);
        assert!(stats.reclaimed_snapshots >= 1);

        // The live line stays fully readable.
        let files = manager.get_actor_workspace(actor.as_str()).unwrap();
        let a = files
            .iter()
            .find(|f| f.path == "a.txt")
            .expect("a.txt present");
        assert_eq!(a.content, b"v3\n".to_vec());

        // Source-index rows go with the pruned commits: only the live
        // line's three edits remain.
        assert_eq!(
            manager.storage_ref().unwrap().count_source_index().unwrap(),
            3
        );

        // A second run over the same store removes nothing.
        assert_eq!(
            manager
                .run_gc_with_grace(GcRetention::default(), Duration::ZERO)
                .unwrap(),
            GcStats::default()
        );
    }

    #[test]
    fn gc_grace_window_protects_fresh_orphans() {
        let manager = manager();
        let actor = manager.actor_id_for("entity-1");
        manager.apply_agent_edit(&actor, "a.txt", b"v1\n").unwrap();

        let git = manager.git_ref().unwrap();
        let tree = git
            .build_tree_from_parent(None, &changes(&[("orphan.txt", b"loose\n")]))
            .unwrap();
        let orphan = git
            .write_commit(&tree, &[], "agent:x", SYSTEM_COMMITTER, 5_000, "orphan")
            .unwrap();

        // A wide grace window keeps the just-written orphan alive.
        let kept = manager
            .run_gc_with_grace(GcRetention::default(), Duration::from_secs(3_600))
            .unwrap();
        assert_eq!(kept.removed_checkpoints, 0);
        assert!(git.read_commit(&orphan).is_ok());

        // A zero grace window reclaims it immediately.
        let reaped = manager
            .run_gc_with_grace(GcRetention::default(), Duration::ZERO)
            .unwrap();
        assert_eq!(reaped.removed_checkpoints, 1);
        assert!(git.read_commit(&orphan).is_err());
    }

    #[test]
    fn gc_keep_recent_heads_protects_newest_orphans() {
        let manager = manager();
        let git = manager.git_ref().unwrap();
        let mut orphans = Vec::new();
        for ts in [1_000i64, 2_000, 3_000] {
            let tree = git
                .build_tree_from_parent(None, &changes(&[("o.txt", ts.to_string().as_bytes())]))
                .unwrap();
            orphans.push(
                git.write_commit(&tree, &[], "orphan", SYSTEM_COMMITTER, ts, "orphan")
                    .unwrap(),
            );
        }

        let stats = manager
            .run_gc_with_grace(GcRetention { keep_recent_heads: 2 }, Duration::ZERO)
            .unwrap();
        assert_eq!(stats.removed_checkpoints, 1);

        // The two newest orphans survive; the oldest is gone.
        assert!(git.read_commit(&orphans[1]).is_ok());
        assert!(git.read_commit(&orphans[2]).is_ok());
        assert!(git.read_commit(&orphans[0]).is_err());
    }

    #[test]
    fn gc_collects_merged_edit_refs() {
        let manager = manager();
        let actor = manager.actor_id_for("entity-1");
        let head = manager.apply_agent_edit(&actor, "a.txt", b"v1\n").unwrap();
        let other = manager.actor_id_for("entity-2");
        let other_head = manager.apply_agent_edit(&other, "b.txt", b"other\n").unwrap();

        let git = manager.git_ref().unwrap();
        let edit_ref = edit_ref_for_actor(actor.as_str());
        // Point main at the first actor's head: that line is fully merged.
        git.write_ref(REF_MAIN, &head).unwrap();

        let stats = manager
            .run_gc_with_grace(GcRetention::default(), Duration::ZERO)
            .unwrap();
        assert_eq!(stats.removed_edit_refs, 1);
        assert_eq!(
            stats.removed_checkpoints, 0,
            "everything is still ref-reachable"
        );
        assert!(
            git.read_ref(&edit_ref).unwrap().is_none(),
            "merged edit ref dropped"
        );
        assert_eq!(
            git.read_ref(&edit_ref_for_actor(other.as_str()))
                .unwrap()
                .as_deref(),
            Some(other_head.as_str())
        );
    }
}
