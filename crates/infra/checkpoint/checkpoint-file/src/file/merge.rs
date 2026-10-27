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

/// Options for merging features into main.
///
/// Multi-feature merges resolve overlapping lines in input order, so the
/// order is semantics. `confirm_order` makes that choice explicit for
/// multi-feature merges; the legacy [`Self::merge_features_to_main`]
/// entry preserves the historical behavior by confirming internally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeatureMergeOptions {
    /// Pre-validate every feature before executing any merge. A predicted
    /// conflict aborts with no main commit created (all-or-nothing for
    /// the predictable cases). Mid-execution races can still partially
    /// advance main; those surface as `PartialMerge` with the completed
    /// list instead of a bare conflict.
    pub atomic: bool,
    /// Asserts the input order is intentional. Required when merging more
    /// than one feature through [`Self::merge_features_to_main_with_options`].
    pub confirm_order: bool,
}

/// Read-only prediction for one feature merge into main. Produced by
/// [`Self::preview_merge_features_to_main`]; no ref is moved and no
/// commit is created.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeatureMergePreview {
    /// Feature name as given.
    pub feature: String,
    /// Whether the merge is predicted to conflict.
    pub would_conflict: bool,
    /// Predicted conflict files (empty when clean).
    pub conflict_files: Vec<String>,
    /// Why the merge cannot proceed even before content comparison
    /// (missing feature, unresolved state). `None` when addressable.
    pub reason: Option<String>,
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
    ///
    /// Legacy entry: preserves the historical non-atomic behavior. For an
    /// explicit order confirmation, a dry-run preview, or all-or-nothing
    /// pre-validation, use [`Self::merge_features_to_main_with_options`]
    /// with [`FeatureMergeOptions`].
    pub fn merge_features_to_main(
        &self,
        feature_names: &[&str],
    ) -> Result<MergeCommitResult, CheckpointError> {
        self.merge_features_to_main_with_options(
            feature_names,
            &FeatureMergeOptions {
                atomic: false,
                confirm_order: true,
            },
        )
    }

    /// Merge features into main with explicit options. Multi-feature
    /// merges require `options.confirm_order` because overlapping lines
    /// resolve in input order. With `options.atomic`, every feature is
    /// pre-validated first and a predicted conflict aborts before any
    /// main commit is created; a conflict surfacing mid-sequence (after
    /// earlier merges already landed, e.g. under concurrent writers)
    /// surfaces as `PartialMerge` carrying the completed features.
    pub fn merge_features_to_main_with_options(
        &self,
        feature_names: &[&str],
        options: &FeatureMergeOptions,
    ) -> Result<MergeCommitResult, CheckpointError> {
        for feature in feature_names {
            crate::branch::ensure_feature_branch_name(feature)?;
        }
        if feature_names.is_empty() {
            return Err(CheckpointError::Validation {
                reason: "no features given to merge".to_string(),
            });
        }
        if feature_names.len() > 1 {
            if !options.confirm_order {
                return Err(CheckpointError::Validation {
                    reason: format!(
                        "merging {} features resolves overlapping lines in input order; pass confirm_order to assert the order is intentional",
                        feature_names.len()
                    ),
                });
            }
            tracing::warn!(
                features = feature_names.len(),
                "merging multiple features sequentially; overlapping lines resolve in input order"
            );
        }
        if options.atomic {
            let previews = self.preview_merge_features_to_main(feature_names)?;
            let blocked: Vec<&FeatureMergePreview> = previews
                .iter()
                .filter(|p| p.would_conflict || p.reason.is_some())
                .collect();
            if !blocked.is_empty() {
                let mut files: Vec<String> = blocked
                    .iter()
                    .flat_map(|p| p.conflict_files.clone())
                    .collect();
                files.sort();
                files.dedup();
                let reasons: Vec<String> =
                    blocked.iter().filter_map(|p| p.reason.clone()).collect();
                if !reasons.is_empty() {
                    return Err(CheckpointError::Validation {
                        reason: format!(
                            "atomic merge pre-validation failed: {}",
                            reasons.join("; ")
                        ),
                    });
                }
                return Err(CheckpointError::MergeConflict {
                    actor: "merge".to_string(),
                    files,
                });
            }
        }
        let mut last: Option<GitMergeOutcome> = None;
        let mut completed: Vec<String> = Vec::new();
        for feature in feature_names {
            // Sequential merges use the caller's actor label when one is
            // known; merges are content-addressed so the label only feeds
            // the commit trailer.
            match self.merge_feature_into_main(feature, "merge") {
                Ok(outcome) => {
                    completed.push((*feature).to_string());
                    last = Some(outcome);
                }
                Err(CheckpointError::MergeConflict { actor, files })
                    if options.atomic && !completed.is_empty() =>
                {
                    return Err(CheckpointError::PartialMerge {
                        completed,
                        actor,
                        files,
                    });
                }
                Err(err) => return Err(err),
            }
        }
        let outcome = last.ok_or_else(|| CheckpointError::Validation {
            reason: "no features given to merge".to_string(),
        })?;
        Ok(MergeCommitResult {
            checkpoint_id: outcome.commit_id.clone(),
            merge_result: outcome,
        })
    }

    /// Dry-run preview for merging features into main: predicts per-feature
    /// conflicts without moving any ref or creating any commit. Confirm
    /// the preview, then execute with
    /// [`Self::merge_features_to_main_with_options`].
    pub fn preview_merge_features_to_main(
        &self,
        feature_names: &[&str],
    ) -> Result<Vec<FeatureMergePreview>, CheckpointError> {
        use crate::git_store::{
            merge_file_maps, REF_MAIN, STATE_CONFLICT_UNRESOLVED, TRAILER_STATE,
        };
        for feature in feature_names {
            crate::branch::ensure_feature_branch_name(feature)?;
        }
        let git = self.git_ref()?;
        let main_head = git.read_ref(REF_MAIN).map_err(map_git_error)?;
        let main_unresolved = match &main_head {
            Some(head) => git
                .read_commit(head)
                .map_err(map_git_error)
                .map(|c| c.trailer(TRAILER_STATE).as_deref() == Some(STATE_CONFLICT_UNRESOLVED))
                .unwrap_or(false),
            None => false,
        };
        let main_tree: std::collections::HashMap<String, Vec<u8>> = match &main_head {
            Some(head) => {
                let commit = git.read_commit(head).map_err(map_git_error)?;
                git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
            }
            None => std::collections::HashMap::new(),
        };
        let mut previews = Vec::with_capacity(feature_names.len());
        for feature in feature_names {
            let feature_ref = feat_ref_for_name(feature);
            let feature_head = git.read_ref(&feature_ref).map_err(map_git_error)?;
            let Some(feature_head) = feature_head else {
                previews.push(FeatureMergePreview {
                    feature: (*feature).to_string(),
                    would_conflict: true,
                    conflict_files: Vec::new(),
                    reason: Some(format!("feature '{feature}' not found")),
                });
                continue;
            };
            let feature_commit = git.read_commit(&feature_head).map_err(map_git_error)?;
            if feature_commit.trailer(TRAILER_STATE).as_deref() == Some(STATE_CONFLICT_UNRESOLVED) {
                previews.push(FeatureMergePreview {
                    feature: (*feature).to_string(),
                    would_conflict: true,
                    conflict_files: feature_commit
                        .trailers(crate::git_store::TRAILER_CONFLICT_FILE),
                    reason: Some(format!(
                        "feature '{feature}' carries an unresolved conflict"
                    )),
                });
                continue;
            }
            if main_unresolved {
                previews.push(FeatureMergePreview {
                    feature: (*feature).to_string(),
                    would_conflict: true,
                    conflict_files: Vec::new(),
                    reason: Some("main carries an unresolved conflict".to_string()),
                });
                continue;
            }
            let feature_tree = git
                .tree_to_bytes(&feature_commit.tree)
                .map_err(map_git_error)?;
            let base_tree: std::collections::HashMap<String, Vec<u8>> = match &main_head {
                Some(main) => match git.merge_base(main, &feature_head).map_err(map_git_error)? {
                    Some(base) => {
                        let commit = git.read_commit(&base).map_err(map_git_error)?;
                        git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
                    }
                    None => std::collections::HashMap::new(),
                },
                None => std::collections::HashMap::new(),
            };
            let (_, conflicts) = merge_file_maps(&base_tree, &main_tree, &feature_tree);
            previews.push(FeatureMergePreview {
                feature: (*feature).to_string(),
                would_conflict: !conflicts.is_empty(),
                conflict_files: conflicts,
                reason: None,
            });
        }
        Ok(previews)
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
    ///
    /// Canonical history-append name is [`Self::revert_feature_merge`];
    /// this `rollback_*` spelling is a legacy alias. For stable
    /// addressing by feature name (survives repeated merges), see
    /// [`Self::revert_feature_by_name`].
    pub fn rollback_feature_from_main(
        &self,
        feature_head: &str,
        actor_str: &str,
    ) -> Result<String, CheckpointError> {
        self.revert_feature_merge(feature_head, actor_str)
    }

    /// Canonical history-append revert: roll back one merged feature from
    /// main by appending a revert commit. Main only moves forward; other
    /// merged features are untouched. Returns the revert commit id.
    pub fn revert_feature_merge(
        &self,
        feature_head: &str,
        actor_str: &str,
    ) -> Result<String, CheckpointError> {
        Ok(self
            .revert_feature_on_main(feature_head, actor_str)?
            .commit_id)
    }

    /// Revert the latest merge of a feature by stable feature name.
    /// The absorbing main merge is located through the `Wf-Feature`
    /// trailer written at merge time, so repeated merges of one feature
    /// stay addressable without the caller tracking exact commit ids.
    /// When the feature was merged more than once, the newest absorbing
    /// merge is reverted. Returns the revert commit id.
    pub fn revert_feature_by_name(
        &self,
        feature_name: &str,
        actor_str: &str,
    ) -> Result<String, CheckpointError> {
        crate::branch::ensure_feature_branch_name(feature_name)?;
        let git = self.git_ref()?;
        let main_head = git
            .read_ref(crate::git_store::REF_MAIN)
            .map_err(map_git_error)?
            .ok_or_else(|| CheckpointError::Validation {
                reason: "main has no commits to roll back".to_string(),
            })?;
        let absorbing = git
            .log(&main_head, 0)
            .map_err(map_git_error)?
            .into_iter()
            .find(|c| c.trailer(crate::git_store::TRAILER_FEATURE).as_deref() == Some(feature_name))
            .ok_or_else(|| CheckpointError::Validation {
                reason: format!("feature '{feature_name}' has no recorded merge into main"),
            })?;
        let feature_head = match absorbing.parents.as_slice() {
            // Main merges record parents as [pre-merge main, feature];
            // a single-parent absorbing merge means main was empty and
            // the feature itself is the only parent.
            [_, feature, ..] => feature.clone(),
            [only] => only.clone(),
            [] => {
                return Err(CheckpointError::Validation {
                    reason: format!(
                        "feature '{feature_name}' merge has no addressable feature parent"
                    ),
                });
            }
        };
        self.revert_feature_merge(&feature_head, actor_str)
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

    /// Run cleanup and return the report form: reclaimed counts plus the
    /// watermark observed after the sweep. The `GcCompleted` event
    /// publication is shared with [`Self::run_gc`]; this entry only adds
    /// the post-sweep watermark so callers observe storage pressure
    /// without a second walk.
    pub fn run_gc_report(
        &self,
        retention: crate::gc::GcRetention,
    ) -> Result<crate::gc::GcReport, CheckpointError> {
        let stats = self.run_gc(retention)?;
        let watermark_after = self.store_watermark()?;
        Ok(crate::gc::GcReport {
            stats,
            watermark_after,
        })
    }

    /// Point-in-time storage watermark from a full reachability walk.
    /// Tiers loose commits into reachable (ref or baseline anchored) and
    /// reclaimable (unreachable) so dashboards can show water levels and
    /// retention can be tuned. Heavier than the cheap counters; call it
    /// on demand, never per pass.
    pub fn store_watermark(&self) -> Result<crate::gc::StoreWatermark, CheckpointError> {
        use std::collections::HashSet;
        let git = self.git_ref()?;
        let mut reachable: HashSet<String> = HashSet::new();
        for commit in git.all_commits().map_err(map_git_error)? {
            Self::insert_commit_closure(git, &commit.id, &commit.tree, &mut reachable);
        }
        if let Some(anchor) = git.baseline_anchor_id() {
            reachable.insert(anchor);
        }
        let ids = Self::list_object_ids(git);
        let mut reachable_commits = 0u64;
        let mut reclaimable_commits = 0u64;
        for id in &ids {
            if git.read_commit(id).is_err() {
                continue;
            }
            if reachable.contains(id) {
                reachable_commits += 1;
            } else {
                reclaimable_commits += 1;
            }
        }
        let last_gc_ms = self
            .storage_ref()
            .ok()
            .and_then(|storage| {
                crate::storage::MetadataStore::load_metadata(storage, crate::gc::LAST_GC_KEY)
                    .ok()
                    .flatten()
            })
            .and_then(|value| value.parse::<i64>().ok());
        Ok(crate::gc::StoreWatermark {
            loose_objects: ids.len() as u64,
            reachable_commits,
            reclaimable_commits,
            last_gc_ms,
        })
    }

    /// Count of loose commits unreachable from every ref and the baseline
    /// anchor: what the next sweep outside the grace window would reclaim.
    /// Cheap enough for dashboards; the full tier split lives in
    /// [`Self::store_watermark`].
    pub fn estimate_reclaimable_commits(&self) -> Result<u64, CheckpointError> {
        Ok(self.store_watermark()?.reclaimable_commits)
    }

    /// Drop loose objects unreachable from any ref, protecting the newest
    /// retained commits. Unreachable objects younger than `grace` are kept:
    /// they may belong to an in-flight commit whose ref update has not
    /// landed yet. Returns pruned commit, tree and blob counts.
    ///
    /// Baseline safety: the pinned anchor is added to the root set so the
    /// alternate chain is never treated as garbage; enumeration covers only
    /// the local objects directory, never the borrowed database; packing
    /// only folds locally listed ids, so borrowed objects stay out.
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
        if let Some(anchor) = git.baseline_anchor_id() {
            reachable.insert(anchor.clone());
            if let Ok(commit) = git.read_commit(&anchor) {
                Self::insert_commit_closure(git, &commit.id, &commit.tree, &mut reachable);
            }
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
        // Local objects only: the loose store handle points at the local
        // objects directory, never at the borrowed database behind the
        // alternates entry, so borrowed history is invisible to the sweep.
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
    /// way, and any failure simply leaves the store unpacked. Only locally
    /// listed loose ids are fed, so borrowed objects behind the alternates
    /// entry are never pulled into the pack. Only objects still present at
    /// pack time are deleted, so a concurrent writer never loses an object.
    fn pack_loose_objects(&self) -> Option<usize> {
        use std::io::Write as _;
        let git = self.git_ref().ok()?;
        let ids = Self::list_object_ids(git);
        if ids.len() < Self::PACK_THRESHOLD {
            return Some(0);
        }
        let git_dir = git.git_dir();
        let staging = tempfile::tempdir().ok()?;
        let pack_base = staging.path().join("pack");
        let mut child = std::process::Command::new("git")
            .arg("--git-dir")
            .arg(git_dir)
            .arg("pack-objects")
            .arg("-q")
            .arg(&pack_base)
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
        manager
            .apply_agent_edit(&other, "b.txt", b"gone\n")
            .unwrap();
        let other_ref = edit_ref_for_actor(other.as_str());
        assert!(manager
            .git_ref()
            .unwrap()
            .read_ref(&other_ref)
            .unwrap()
            .is_some());
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
            .run_gc_with_grace(
                GcRetention {
                    keep_recent_heads: 2,
                },
                Duration::ZERO,
            )
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
        let other_head = manager
            .apply_agent_edit(&other, "b.txt", b"other\n")
            .unwrap();

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
