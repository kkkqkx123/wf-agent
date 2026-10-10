//! Approval orchestration on the commit DAG.
//!
//! Write domain: submit, approve, reject and merge review refs into features.
//! Read models live in the top-level approval module.
use std::collections::HashMap;
use std::path::Path;

use wf_types::config::file_checkpoint::ConflictBehavior;

use crate::approval::{to_conflict_views, MergeOutcome, PendingApproval};
use crate::event::CheckpointEventBus;
use crate::file::git_merge::{conflict_details, GitMergeOutcome};
use crate::file::git_write::map_git_error;
use crate::file::merge::MergeCommitResult;
use crate::file::util::{resolve_restore_target, sha256_hex, validate_workspace_relative_path};
use crate::file::FileCheckpointManager;
use crate::git_store::{feat_ref_for_name, REF_REVIEW_PREFIX};
use crate::provenance::DeltaSummary;
use crate::storage::ReviewStatus;
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    // ── approval layer (list / approve / reject) ─────────────────────

    /// All pending approvals: review refs whose explicit state is
    /// `pending`. Persisted in SQLite, so pending approvals survive across
    /// executions ("review after the run ends"). Refs without any state row
    /// are half-submitted leftovers: treated as unsubmitted, skipped with a
    /// warning instead of surfacing half-state.
    pub fn list_pending_approvals(&self) -> Result<Vec<PendingApproval>, CheckpointError> {
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut views = Vec::new();
        for (review_ref, _) in git.list_refs(REF_REVIEW_PREFIX).map_err(map_git_error)? {
            match storage.get_review_state(&review_ref)? {
                Some(ReviewStatus::Pending) => {}
                None => {
                    tracing::warn!(review = %review_ref, "pending scan found ref without state row; skipping");
                    continue;
                }
                _ => continue,
            }
            let Some(head) = git.read_ref(&review_ref).map_err(map_git_error)? else {
                tracing::warn!(review = %review_ref, "pending review ref has no head; skipping");
                continue;
            };
            let Ok(commit) = git.read_commit(&head) else {
                tracing::warn!(review = %review_ref, head = %head, "pending review head unreadable; skipping");
                continue;
            };
            let actor = commit
                .trailer(crate::git_store::TRAILER_ACTOR)
                .unwrap_or_default();
            let files = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
            let mut changes: Vec<DeltaSummary> = files
                .iter()
                .map(|(path, bytes)| DeltaSummary {
                    file: path.clone(),
                    source: actor.clone(),
                    timestamp: commit.committer_ts,
                    snapshot_id: head.clone(),
                    hash: sha256_hex(bytes),
                    message: None,
                })
                .collect();
            changes.sort_by(|a, b| a.file.cmp(&b.file));
            views.push(PendingApproval {
                actor,
                snapshot_id: head,
                submitted_at: commit.committer_ts,
                changes,
            });
        }
        views.sort_by_key(|a| a.submitted_at);
        Ok(views)
    }

    /// Sweep half-rejected leftovers for an actor: pending state rows whose
    /// review ref no longer exists (a previous reject deleted the ref but
    /// failed before deleting the row). The rows are deleted and counted so
    /// a repeated reject converges instead of leaving invisible pending
    /// state behind. Only rows under this actor's review-ref prefix are
    /// touched, matching the submission naming scheme.
    pub fn reconcile_reviews(&self, entity_id: &str) -> Result<usize, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let prefix = format!(
            "{REF_REVIEW_PREFIX}{}-",
            crate::git_store::sanitize_ref_component(actor.as_str())
        );
        let mut swept = 0usize;
        for (review_ref, status) in storage.list_review_states()? {
            if status != ReviewStatus::Pending || !review_ref.starts_with(&prefix) {
                continue;
            }
            let headless = git.read_ref(&review_ref).map_err(map_git_error)?.is_none();
            if headless {
                tracing::warn!(review = %review_ref, "reconciling orphan pending state row; ref already gone");
                storage.delete_review_state(&review_ref)?;
                swept += 1;
            }
        }
        Ok(swept)
    }

    /// Reject a pending approval: delete the actor's pending review refs
    /// and their state rows. Merged history is never touched. Returns the
    /// last deleted review commit id (hex).
    ///
    /// Deletion order is fixed (ref first, then state row) and orphan state
    /// rows are swept up front, so a repeated call after a mid-loop failure
    /// converges to the same result instead of stalling on half-state.
    ///
    /// `reason` is an optional human-readable rejection reason used only for
    /// logging and diagnostics; it never changes the rollback semantics and
    /// is never persisted to storage.
    pub fn reject_changes(
        &self,
        entity_id: &str,
        reason: Option<&str>,
    ) -> Result<String, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let swept = self.reconcile_reviews(entity_id)?;
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut targets: Vec<(String, String)> = Vec::new();
        for (review_ref, _) in git.list_refs(REF_REVIEW_PREFIX).map_err(map_git_error)? {
            if storage.get_review_state(&review_ref)? != Some(ReviewStatus::Pending) {
                continue;
            }
            let Some(head) = git.read_ref(&review_ref).map_err(map_git_error)? else {
                tracing::warn!(review = %review_ref, "reject scan found headless review; skipping");
                continue;
            };
            let Ok(commit) = git.read_commit(&head) else {
                tracing::warn!(review = %review_ref, "reject scan found unreadable review; skipping");
                continue;
            };
            if commit.trailer(crate::git_store::TRAILER_ACTOR).as_deref() != Some(actor.as_str()) {
                continue;
            }
            targets.push((review_ref, head));
        }
        let mut deleted: Vec<String> = Vec::new();
        for (review_ref, head) in targets {
            git.delete_ref(&review_ref).map_err(map_git_error)?;
            storage.delete_review_state(&review_ref)?;
            deleted.push(head);
        }
        let last = deleted
            .into_iter()
            .next_back()
            .ok_or_else(|| CheckpointError::Validation {
                reason: format!("actor '{}' has no pending submission", actor.as_str()),
            })?;
        match reason.map(str::trim).filter(|r| !r.is_empty()) {
            Some(reason) => tracing::info!(
                entity = %entity_id,
                baseline = %last,
                reason = %reason,
                reconciled_orphans = swept,
                "rejected pending approval",
            ),
            None => tracing::info!(
                entity = %entity_id,
                baseline = %last,
                reconciled_orphans = swept,
                "rejected pending approval without a reason",
            ),
        }
        Ok(last)
    }

    /// Approve an actor's pending changes: submit to a review ref, then
    /// merge into the named feature with a true multi-parent merge.
    ///
    /// `paths` selects the file-level approval mode:
    /// - `None`: approve the whole submission (full batch merge).
    /// - `Some(paths)`: commit only the listed files from the reviewed
    ///   tree onto the feature ref (content taken verbatim — no merge,
    ///   the reviewed content is authoritative); every approved file
    ///   publishes a `FileChanged` event.
    ///
    /// - `ConflictBehavior::Marker` (default): conflicted merges keep
    ///   standard markers in the affected files (written to
    ///   `workspace_root` when provided) and the outcome reports them;
    ///   execution continues.
    /// - `ConflictBehavior::Fail`: any conflict aborts with an error.
    /// - `ConflictBehavior::Approval`: conflicts stay pending on the review
    ///   ref instead of being merged.
    pub fn approve_changes(
        &self,
        entity_id: &str,
        feature_name: &str,
        paths: Option<Vec<String>>,
        conflict_behavior: ConflictBehavior,
        workspace_root: Option<&Path>,
    ) -> Result<MergeOutcome, CheckpointError> {
        crate::branch::ensure_feature_branch_name(feature_name)?;
        let actor = self.actor_id_for(entity_id);
        if let Some(ref selected) = paths {
            for path in selected {
                validate_workspace_relative_path(path)?;
            }
            if selected.is_empty() {
                let snapshot_id = self
                    .newest_pending_review(actor.as_str())?
                    .map(|(_, head)| head)
                    .or(self.latest_checkpoint_id(&actor)?)
                    .unwrap_or_default();
                return Ok(MergeOutcome {
                    merged: false,
                    snapshot_id,
                    conflicts: vec![],
                    conflict_files: vec![],
                    message: "no paths selected; changes remain pending".to_string(),
                });
            }
        }
        // Every approval starts from a fresh submission: edit to review is
        // a ref copy, never a merge.
        let (review_ref, review_head) = self.submit_for_review(entity_id)?;

        // File-level approval: three-way merge only the selected paths from
        // the reviewed tree onto the feature ref, leaving the review pending
        // for the rest. Unselected paths keep the feature content, so concurrent
        // feature changes outside the selection are never overwritten.
        if let Some(paths) = paths {
            if conflict_behavior == ConflictBehavior::Approval {
                // Consistent with full-batch approval: keep the submission
                // pending and let the host resolve it.
                return Ok(MergeOutcome {
                    merged: false,
                    snapshot_id: review_head,
                    conflicts: vec![],
                    conflict_files: vec![],
                    message: "changes remain pending on the review ref".to_string(),
                });
            }
            let mut validated_paths: Vec<String> = Vec::with_capacity(paths.len());
            for path in &paths {
                validated_paths.push(validate_workspace_relative_path(path)?);
            }
            validated_paths.sort();
            validated_paths.dedup();
            let git = self.git_ref()?;
            let storage = self.storage_ref()?;
            let feature_ref = feat_ref_for_name(feature_name);
            if let Some(head) = git.read_ref(&feature_ref).map_err(map_git_error)? {
                if let Ok(commit) = git.read_commit(&head) {
                    if commit.trailer(crate::git_store::TRAILER_STATE).as_deref()
                        == Some(crate::git_store::STATE_CONFLICT_UNRESOLVED)
                    {
                        return Err(CheckpointError::MergeConflict {
                            actor: actor.as_str().to_string(),
                            files: commit.trailers(crate::git_store::TRAILER_CONFLICT_FILE),
                        });
                    }
                }
            }
            let review_commit = git.read_commit(&review_head).map_err(map_git_error)?;
            let review_tree = git
                .tree_to_bytes(&review_commit.tree)
                .map_err(map_git_error)?;
            let feature_head = git.read_ref(&feature_ref).map_err(map_git_error)?;
            let feature_tree: HashMap<String, Vec<u8>> = match &feature_head {
                Some(head) => {
                    let commit = git.read_commit(head).map_err(map_git_error)?;
                    git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
                }
                None => HashMap::new(),
            };
            let base_tree: HashMap<String, Vec<u8>> = match &feature_head {
                Some(feature) => match git
                    .merge_base(feature, &review_head)
                    .map_err(map_git_error)?
                {
                    Some(base) => {
                        let commit = git.read_commit(&base).map_err(map_git_error)?;
                        git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
                    }
                    None => HashMap::new(),
                },
                None => HashMap::new(),
            };
            let mut merged_changes: HashMap<String, Option<Vec<u8>>> = HashMap::new();
            let mut conflict_files: Vec<String> = Vec::new();
            let mut approved: Vec<String> = Vec::new();
            for path in &validated_paths {
                let base = base_tree.get(path).map(|v| v.as_slice());
                let ours = feature_tree.get(path).map(|v| v.as_slice());
                let theirs = review_tree.get(path).map(|v| v.as_slice());
                // Path untouched on both sides relative to base: nothing to approve.
                if ours == theirs {
                    continue;
                }
                let outcome = crate::git_store::merge_file_contents(base, ours, theirs);
                if outcome.conflicted {
                    conflict_files.push(path.clone());
                }
                merged_changes.insert(path.clone(), outcome.bytes);
                approved.push(path.clone());
            }
            if approved.is_empty() {
                return Ok(MergeOutcome {
                    merged: false,
                    snapshot_id: review_head,
                    conflicts: vec![],
                    conflict_files: vec![],
                    message: "no approved files; changes remain pending".to_string(),
                });
            }
            if !conflict_files.is_empty() && conflict_behavior == ConflictBehavior::Fail {
                return Err(CheckpointError::MergeConflict {
                    actor: actor.as_str().to_string(),
                    files: {
                        let mut files = conflict_files.clone();
                        files.sort();
                        files
                    },
                });
            }
            let mut staged: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
            for (path, content) in &merged_changes {
                staged.insert(
                    path.clone(),
                    content
                        .clone()
                        .map(|bytes| (crate::git_store::MODE_FILE.to_string(), bytes)),
                );
            }
            let mut commit_paths: Vec<String> = merged_changes.keys().cloned().collect();
            commit_paths.sort();
            let message = crate::git_store::commit_message(
                "file-level approval",
                Some(actor.as_str()),
                None,
                Some("review"),
                &[],
            );
            let commit_outcome = git
                .commit_on_ref(&feature_ref, &staged, actor.as_str(), &message)
                .map_err(map_git_error)?;
            if commit_outcome.created {
                self.index_commit(
                    storage,
                    &commit_outcome.id,
                    actor.as_str(),
                    "",
                    "review",
                    &commit_paths,
                )?;
                for path in &commit_paths {
                    let bytes = merged_changes.get(path).and_then(|c| c.as_deref());
                    self.publish_file_event(&commit_outcome.id, path, actor.as_str(), bytes);
                }
            }
            let head = git
                .read_ref(&feature_ref)
                .map_err(map_git_error)?
                .unwrap_or(commit_outcome.id.clone());
            if !conflict_files.is_empty() {
                conflict_files.sort();
                if conflict_behavior == ConflictBehavior::Marker {
                    if let Some(root) = workspace_root {
                        for file in &conflict_files {
                            let target = match resolve_restore_target(root, file) {
                                Ok(v) => v,
                                Err(err) => {
                                    tracing::warn!("marker write skipped for '{file}': {err}");
                                    continue;
                                }
                            };
                            if let Some(parent) = target.parent() {
                                if let Err(err) = std::fs::create_dir_all(parent) {
                                    tracing::warn!("marker write skipped for '{file}': {err}");
                                    continue;
                                }
                            }
                            let Ok(commit) = git.read_commit(&head) else {
                                tracing::warn!(
                                    "marker write skipped for '{file}': head unreadable"
                                );
                                continue;
                            };
                            let Ok(files) = git.tree_to_bytes(&commit.tree) else {
                                tracing::warn!(
                                    "marker write skipped for '{file}': tree unreadable"
                                );
                                continue;
                            };
                            if let Some(bytes) = files.get(file) {
                                if let Err(err) = std::fs::write(&target, bytes) {
                                    tracing::warn!("marker write skipped for '{file}': {err}");
                                }
                            }
                        }
                    }
                    if let Some(ref bus) = self.event_bus {
                        bus.publish(CheckpointEventBus::merge_conflicted(
                            head.clone(),
                            conflict_files.clone(),
                            Some(actor.as_str().to_string()),
                        ));
                    }
                }
                let conflicts =
                    to_conflict_views(&conflict_details(&merged_changes, &conflict_files));
                return Ok(MergeOutcome {
                    merged: true,
                    snapshot_id: head,
                    conflicts,
                    conflict_files,
                    message: format!(
                        "approved {} file(s) with conflicts ({}); others remain pending",
                        approved.len(),
                        approved.join(", ")
                    ),
                });
            }
            return Ok(MergeOutcome {
                merged: true,
                snapshot_id: head,
                conflicts: vec![],
                conflict_files: vec![],
                message: format!(
                    "approved {} file(s) ({}); others remain pending",
                    approved.len(),
                    approved.join(", ")
                ),
            });
        }

        if conflict_behavior == ConflictBehavior::Approval {
            // Keep the submission pending and let the host resolve it.
            return Ok(MergeOutcome {
                merged: false,
                snapshot_id: review_head,
                conflicts: vec![],
                conflict_files: vec![],
                message: "changes remain pending on the review ref".to_string(),
            });
        }

        if conflict_behavior == ConflictBehavior::Fail {
            let git = self.git_ref()?;
            let feature_ref = feat_ref_for_name(feature_name);
            if let Some(head) = git.read_ref(&feature_ref).map_err(map_git_error)? {
                if let Ok(commit) = git.read_commit(&head) {
                    if commit.trailer(crate::git_store::TRAILER_STATE).as_deref()
                        == Some(crate::git_store::STATE_CONFLICT_UNRESOLVED)
                    {
                        return Err(CheckpointError::MergeConflict {
                            actor: actor.as_str().to_string(),
                            files: commit.trailers(crate::git_store::TRAILER_CONFLICT_FILE),
                        });
                    }
                }
            }
            let review_commit = git.read_commit(&review_head).map_err(map_git_error)?;
            let review_tree = git
                .tree_to_bytes(&review_commit.tree)
                .map_err(map_git_error)?;
            let feature_head = git.read_ref(&feature_ref).map_err(map_git_error)?;
            let feature_tree: std::collections::HashMap<String, Vec<u8>> = match &feature_head {
                Some(head) => {
                    let commit = git.read_commit(head).map_err(map_git_error)?;
                    git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
                }
                None => std::collections::HashMap::new(),
            };
            let base_tree: std::collections::HashMap<String, Vec<u8>> = match &feature_head {
                Some(feature) => match git
                    .merge_base(feature, &review_head)
                    .map_err(map_git_error)?
                {
                    Some(base) => {
                        let commit = git.read_commit(&base).map_err(map_git_error)?;
                        git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
                    }
                    None => std::collections::HashMap::new(),
                },
                None => std::collections::HashMap::new(),
            };
            let (_, predicted) =
                crate::git_store::merge_file_maps(&base_tree, &feature_tree, &review_tree);
            if !predicted.is_empty() {
                let mut files = predicted;
                files.sort();
                return Err(CheckpointError::MergeConflict {
                    actor: actor.as_str().to_string(),
                    files,
                });
            }
        }

        match self.merge_review_into_feature(
            &review_ref,
            &review_head,
            feature_name,
            actor.as_str(),
        ) {
            Ok(merged) => {
                let conflicts = to_conflict_views(&merged.details);
                let mut conflict_files = merged.conflict_files.clone();
                conflict_files.sort();
                if !conflicts.is_empty() && conflict_behavior == ConflictBehavior::Fail {
                    return Err(CheckpointError::MergeConflict {
                        actor: actor.as_str().to_string(),
                        files: conflict_files,
                    });
                }
                if !conflicts.is_empty() && conflict_behavior == ConflictBehavior::Marker {
                    if let Some(root) = workspace_root {
                        for detail in &merged.details {
                            let target = match resolve_restore_target(root, &detail.file) {
                                Ok(v) => v,
                                Err(err) => {
                                    tracing::warn!(
                                        "marker write skipped for '{}': {err}",
                                        detail.file
                                    );
                                    continue;
                                }
                            };
                            if let Some(parent) = target.parent() {
                                if let Err(err) = std::fs::create_dir_all(parent) {
                                    tracing::warn!(
                                        "marker write skipped for '{}': {err}",
                                        detail.file
                                    );
                                    continue;
                                }
                            }
                            let Ok(git) = self.git_ref() else {
                                tracing::warn!(
                                    "marker write skipped for '{}': store unavailable",
                                    detail.file
                                );
                                continue;
                            };
                            let Ok(commit) = git.read_commit(&merged.commit_id) else {
                                tracing::warn!(
                                    "marker write skipped for '{}': commit unreadable",
                                    detail.file
                                );
                                continue;
                            };
                            let Ok(files) = git.tree_to_bytes(&commit.tree) else {
                                tracing::warn!(
                                    "marker write skipped for '{}': tree unreadable",
                                    detail.file
                                );
                                continue;
                            };
                            if let Some(bytes) = files.get(&detail.file) {
                                if let Err(err) = std::fs::write(&target, bytes) {
                                    tracing::warn!(
                                        "marker write skipped for '{}': {err}",
                                        detail.file
                                    );
                                }
                            }
                        }
                    }
                    if let Some(ref bus) = self.event_bus {
                        bus.publish(CheckpointEventBus::merge_conflicted(
                            merged.commit_id.clone(),
                            conflict_files.clone(),
                            Some(actor.as_str().to_string()),
                        ));
                    }
                }
                Ok(MergeOutcome {
                    merged: true,
                    snapshot_id: merged.commit_id,
                    conflicts,
                    conflict_files,
                    message: "merged with conflicts".to_string(),
                })
            }
            Err(CheckpointError::MergeConflict { files, .. }) => {
                if conflict_behavior == ConflictBehavior::Fail {
                    return Err(CheckpointError::MergeConflict {
                        actor: actor.as_str().to_string(),
                        files,
                    });
                }
                Ok(MergeOutcome {
                    merged: false,
                    snapshot_id: review_head,
                    conflicts: vec![],
                    conflict_files: files,
                    message: "merge blocked by an unresolved feature conflict".to_string(),
                })
            }
            Err(err) => Err(err),
        }
    }

    /// Full merge entry point for an actor: submit to review, then merge
    /// into the named feature (the `ApprovalPolicy::auto` path). The merge
    /// commit itself is the multi-parent record linking the feature head
    /// and the review commit.
    pub fn merge_entity_changes(
        &self,
        entity_id: &str,
        feature_name: &str,
    ) -> Result<MergeCommitResult, CheckpointError> {
        crate::branch::ensure_feature_branch_name(feature_name)?;
        let actor = self.actor_id_for(entity_id);
        let (review_ref, review_head) = self.submit_for_review(entity_id)?;
        let merge_result = self.merge_review_into_feature(
            &review_ref,
            &review_head,
            feature_name,
            actor.as_str(),
        )?;
        Ok(MergeCommitResult {
            checkpoint_id: merge_result.commit_id.clone(),
            merge_result,
        })
    }

    /// Resolve conflicts on a feature ref by committing the provided
    /// resolved content as a child of the conflicted head, clearing the
    /// unresolved marker. The resolution is authoritative.
    ///
    /// Returns the number of files that still carry an unresolved conflict
    /// after this operation (0 = fully resolved).
    pub fn resolve_conflicts(
        &self,
        entity_id: &str,
        feature_name: &str,
        resolutions: &[(String, Vec<u8>)],
    ) -> Result<usize, CheckpointError> {
        crate::branch::ensure_feature_branch_name(feature_name)?;
        let actor = self.actor_id_for(entity_id);
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let feature_ref = feat_ref_for_name(feature_name);
        let head = git
            .read_ref(&feature_ref)
            .map_err(map_git_error)?
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("feature '{feature_name}'"),
            })?;
        if resolutions.is_empty() {
            return Err(CheckpointError::Validation {
                reason: "conflict resolutions must not be empty".to_string(),
            });
        }
        let conflicted = {
            let commit = git.read_commit(&head).map_err(map_git_error)?;
            if commit.trailer(crate::git_store::TRAILER_STATE).as_deref()
                == Some(crate::git_store::STATE_CONFLICT_UNRESOLVED)
            {
                commit.trailers(crate::git_store::TRAILER_CONFLICT_FILE)
            } else {
                Vec::new()
            }
        };
        let mut changes: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
        let mut paths: Vec<String> = Vec::new();
        let mut resolved_contents: Vec<(String, Vec<u8>)> = Vec::new();
        for (path, content) in resolutions {
            let validated = validate_workspace_relative_path(path)?;
            paths.push(validated.clone());
            changes.insert(
                validated.clone(),
                Some((crate::git_store::MODE_FILE.to_string(), content.clone())),
            );
            resolved_contents.push((validated, content.clone()));
        }
        paths.sort();
        let parent_tree = Some(git.read_commit(&head).map_err(map_git_error)?.tree);
        let tree = git
            .build_tree_from_parent(parent_tree.as_deref(), &changes)
            .map_err(map_git_error)?;
        let message = crate::git_store::commit_message(
            &format!("resolve conflicts on feature '{feature_name}'"),
            Some(actor.as_str()),
            None,
            Some("review"),
            &[(
                crate::git_store::TRAILER_STATE.to_string(),
                crate::git_store::STATE_CONFLICT_RESOLVED.to_string(),
            )],
        );
        let id = git
            .write_commit(
                &tree,
                std::slice::from_ref(&head),
                actor.as_str(),
                crate::git_store::SYSTEM_COMMITTER,
                self.creation_timestamp()?,
                &message,
            )
            .map_err(map_git_error)?;
        git.compare_and_swap(&feature_ref, Some(head.as_str()), &id)
            .map_err(map_git_error)?;
        self.index_commit(storage, &id, actor.as_str(), "", "review", &paths)?;
        for (path, content) in &resolved_contents {
            self.publish_file_event(&id, path, actor.as_str(), Some(content.as_slice()));
        }
        let resolved: std::collections::HashSet<&str> = paths.iter().map(String::as_str).collect();
        let remaining = conflicted
            .iter()
            .filter(|file| !resolved.contains(file.as_str()))
            .count();
        if remaining == 0 && !conflicted.is_empty() {
            // Fully resolved: advance related pending reviews to approved so
            // the review state machine does not stall in pending. Only reviews
            // reachable from the resolution commit are closed.
            if let Ok(refs) = git.list_refs(REF_REVIEW_PREFIX).map_err(map_git_error) {
                for (review_ref, _) in refs {
                    if storage.get_review_state(&review_ref).unwrap_or(None)
                        != Some(ReviewStatus::Pending)
                    {
                        continue;
                    }
                    if let Some(head_id) = git
                        .read_ref(&review_ref)
                        .map_err(map_git_error)
                        .unwrap_or(None)
                    {
                        if let Ok(commit) = git.read_commit(&head_id) {
                            if commit.trailer(crate::git_store::TRAILER_ACTOR).as_deref()
                                != Some(actor.as_str())
                            {
                                continue;
                            }
                            let related = git
                                .is_ancestor(&head_id, &id)
                                .map_err(map_git_error)
                                .unwrap_or(false);
                            if related {
                                let _ =
                                    storage.set_review_state(&review_ref, ReviewStatus::Approved);
                            }
                        }
                    }
                }
            }
        }
        Ok(remaining)
    }

    // ── end-of-execution approval policy ─────────────────────────────

    /// The configured layered approval policy.
    pub fn approval_policy(&self) -> crate::file::ApprovalPolicy {
        self.policy.approval_policy
    }

    /// The configured merge conflict behavior.
    pub fn conflict_behavior(&self) -> ConflictBehavior {
        self.policy.conflict_behavior
    }

    /// Default feature name merged into under `ApprovalPolicy::auto` (a
    /// per-execution feature keeps actors isolated while still landing the
    /// changes into the mainline layer).
    pub fn default_feature_name(entity_id: &str) -> String {
        format!("exec-{}", entity_id)
    }

    /// Agent loop end hook: applies the configured approval policy.
    ///
    /// - `None`: no-op (the actor's edit ref keeps its history and stays
    ///   queryable).
    /// - `Auto`: submit the actor to review and immediately merge into the
    ///   default feature ref.
    /// - `Llm` / `Manual`: submit the actor to review and leave the changes
    ///   pending (`approve_changes` tool / host API resolve them, possibly
    ///   across executions).
    ///
    /// Best-effort by design: the file checkpoint layer must never break an
    /// execution that finished successfully, so failures are reported but
    /// not propagated.
    pub fn on_agent_complete(
        &self,
        entity_id: &str,
    ) -> Result<Option<GitMergeOutcome>, CheckpointError> {
        match self.policy.approval_policy {
            crate::file::ApprovalPolicy::None => Ok(None),
            crate::file::ApprovalPolicy::Auto => {
                let feature = Self::default_feature_name(entity_id);
                match self.merge_entity_changes(entity_id, &feature) {
                    Ok(merged) => Ok(Some(merged.merge_result)),
                    Err(err) => {
                        tracing::warn!("auto approval failed after successful execution: {err}");
                        Ok(None)
                    }
                }
            }
            crate::file::ApprovalPolicy::Llm | crate::file::ApprovalPolicy::Manual => {
                match self.move_agent_to_approval(entity_id) {
                    Ok(_) => Ok(None),
                    Err(err) => {
                        tracing::warn!(
                            "submit for approval failed after successful execution: {err}"
                        );
                        Ok(None)
                    }
                }
            }
        }
    }

    /// Approve a pending actor's changes using the configured conflict
    /// behavior (thin wrapper over [`Self::approve_changes`]).
    pub fn approve_pending(
        &self,
        entity_id: &str,
        feature_name: &str,
    ) -> Result<MergeOutcome, CheckpointError> {
        self.approve_changes(
            entity_id,
            feature_name,
            None,
            self.policy.conflict_behavior,
            self.workspace_root.as_deref(),
        )
    }

    /// File-level approval: approve only the listed paths from a pending
    /// actor submission, leaving the rest pending on the review ref (thin
    /// wrapper over [`Self::approve_changes`] with `paths`).
    pub fn approve_pending_paths(
        &self,
        entity_id: &str,
        feature_name: &str,
        paths: Vec<String>,
    ) -> Result<MergeOutcome, CheckpointError> {
        self.approve_changes(
            entity_id,
            feature_name,
            Some(paths),
            self.policy.conflict_behavior,
            self.workspace_root.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn orphan_ref(manager: &FileCheckpointManager, entity: &str, tag: &str) -> String {
        let actor = manager.actor_id_for(entity);
        format!(
            "refs/wf/review/{}-{tag}-0",
            crate::git_store::sanitize_ref_component(actor.as_str())
        )
    }

    #[test]
    fn reconcile_sweeps_only_actor_orphans() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let storage = manager.storage_ref().unwrap();
        let alice_row = orphan_ref(&manager, "alice", "7");
        storage
            .set_review_state(&alice_row, ReviewStatus::Pending)
            .unwrap();
        storage
            .set_review_state("refs/wf/review/bob-7-0", ReviewStatus::Pending)
            .unwrap();
        assert_eq!(manager.reconcile_reviews("alice").unwrap(), 1);
        assert_eq!(storage.get_review_state(&alice_row).unwrap(), None);
        assert_eq!(
            storage.get_review_state("refs/wf/review/bob-7-0").unwrap(),
            Some(ReviewStatus::Pending)
        );
    }

    #[test]
    fn reject_converges_after_orphan_sweep() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let storage = manager.storage_ref().unwrap();
        let alice_row = orphan_ref(&manager, "alice", "9");
        storage
            .set_review_state(&alice_row, ReviewStatus::Pending)
            .unwrap();
        let first = manager.reject_changes("alice", None);
        assert!(first.is_err());
        assert_eq!(storage.get_review_state(&alice_row).unwrap(), None);
        let second = manager.reject_changes("alice", None);
        assert!(second.is_err());
    }
}
