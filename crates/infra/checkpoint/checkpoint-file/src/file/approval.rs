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
use crate::file::util::{resolve_restore_target, validate_workspace_relative_path};
use crate::file::FileCheckpointManager;
use crate::git_store::{feat_ref_for_name, REF_REVIEW_PREFIX};
use crate::provenance::DeltaSummary;
use crate::storage::ReviewStatus;
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    // ── approval layer (list / approve / reject) ─────────────────────

    /// All pending approvals: review refs whose explicit state is
    /// `pending`. Persisted in SQLite, so pending approvals survive across
    /// executions ("review after the run ends").
    pub fn list_pending_approvals(&self) -> Result<Vec<PendingApproval>, CheckpointError> {
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut views = Vec::new();
        for (review_ref, _) in git.list_refs(REF_REVIEW_PREFIX).map_err(map_git_error)? {
            if storage.get_review_state(&review_ref)? != Some(ReviewStatus::Pending) {
                continue;
            }
            let Some(head) = git.read_ref(&review_ref).map_err(map_git_error)? else {
                continue;
            };
            let Ok(commit) = git.read_commit(&head) else {
                continue;
            };
            let actor = commit
                .trailer(crate::git_store::TRAILER_ACTOR)
                .unwrap_or_default();
            let files = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
            let mut changes: Vec<DeltaSummary> = files
                .iter()
                .map(|(path, (_, blob))| DeltaSummary {
                    file: path.clone(),
                    source: actor.clone(),
                    timestamp: commit.committer_ts,
                    snapshot_id: head.clone(),
                    hash: blob.clone(),
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

    /// Reject a pending approval: delete the actor's pending review refs
    /// and their state rows. Merged history is never touched. Returns the
    /// last deleted review commit id (hex).
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
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut deleted: Vec<String> = Vec::new();
        for (review_ref, _) in git.list_refs(REF_REVIEW_PREFIX).map_err(map_git_error)? {
            if storage.get_review_state(&review_ref)? != Some(ReviewStatus::Pending) {
                continue;
            }
            let Some(head) = git.read_ref(&review_ref).map_err(map_git_error)? else {
                continue;
            };
            let Ok(commit) = git.read_commit(&head) else {
                continue;
            };
            if commit.trailer(crate::git_store::TRAILER_ACTOR).as_deref() != Some(actor.as_str()) {
                continue;
            }
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
                "rejected pending approval",
            ),
            None => tracing::info!(
                entity = %entity_id,
                baseline = %last,
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
        // Every approval starts from a fresh submission: edit to review is
        // a ref copy, never a merge.
        let (review_ref, review_head) = self.submit_for_review(entity_id)?;

        // File-level approval: three-way merge only the selected paths from
        // the reviewed tree onto the feature ref, leaving the review pending
        // for the rest. Unselected paths keep the feature content, so concurrent
        // feature changes outside the selection are never overwritten.
        if let Some(paths) = paths {
            if paths.is_empty() {
                return Ok(MergeOutcome {
                    merged: false,
                    snapshot_id: review_head,
                    conflicts: vec![],
                    conflict_files: vec![],
                    message: "no paths selected; changes remain pending".to_string(),
                });
            }
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
                            let target = resolve_restore_target(root, file)?;
                            if let Some(parent) = target.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            let commit = git.read_commit(&head).map_err(map_git_error)?;
                            let files = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
                            if let Some(bytes) = files.get(file) {
                                std::fs::write(&target, bytes)?;
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
                            let target = resolve_restore_target(root, &detail.file)?;
                            if let Some(parent) = target.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            let git = self.git_ref()?;
                            let commit =
                                git.read_commit(&merged.commit_id).map_err(map_git_error)?;
                            let files = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
                            if let Some(bytes) = files.get(&detail.file) {
                                std::fs::write(&target, bytes)?;
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
        let conflicted = git
            .read_commit(&head)
            .map(|commit| {
                if commit.trailer(crate::git_store::TRAILER_STATE).as_deref()
                    == Some(crate::git_store::STATE_CONFLICT_UNRESOLVED)
                {
                    commit.trailers(crate::git_store::TRAILER_CONFLICT_FILE)
                } else {
                    Vec::new()
                }
            })
            .unwrap_or_default();
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
        let parent_tree = match git.read_commit(&head).map_err(map_git_error) {
            Ok(commit) => Some(commit.tree),
            Err(err) => {
                tracing::warn!(
                    entity_id = %entity_id,
                    feature = %feature_name,
                    error = %err,
                    "reading feature head commit failed; building tree without a parent"
                );
                None
            }
        };
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
                &[head],
                actor.as_str(),
                crate::git_store::SYSTEM_COMMITTER,
                self.creation_timestamp()?,
                &message,
            )
            .map_err(map_git_error)?;
        git.write_ref(&feature_ref, &id).map_err(map_git_error)?;
        self.index_commit(storage, &id, actor.as_str(), "", "review", &paths)?;
        for (path, content) in &resolved_contents {
            self.publish_file_event(&id, path, actor.as_str(), Some(content.as_slice()));
        }
        let resolved: std::collections::HashSet<&str> = paths.iter().map(String::as_str).collect();
        let remaining = conflicted
            .iter()
            .filter(|file| !resolved.contains(file.as_str()))
            .count();
        if remaining == 0 {
            // Fully resolved: advance related pending reviews to approved so
            // the review state machine does not stall in pending.
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
                                == Some(actor.as_str())
                            {
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
                let merged = self.merge_entity_changes(entity_id, &feature)?;
                Ok(Some(merged.merge_result))
            }
            crate::file::ApprovalPolicy::Llm | crate::file::ApprovalPolicy::Manual => {
                self.move_agent_to_approval(entity_id)?;
                Ok(None)
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
