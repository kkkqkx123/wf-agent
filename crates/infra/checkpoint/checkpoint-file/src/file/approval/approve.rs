//! Approval orchestration on the commit DAG.
//!
//! Write domain: submit, approve, reject and merge review refs into features.
//! Read models live in the top-level approval module.
use std::collections::HashMap;
use std::path::Path;

use wf_types::config::file_checkpoint::ConflictBehavior;

use crate::approval::{to_conflict_views, MergeOutcome};
use crate::event::{ApprovalDecision, CheckpointEventBus};
use crate::file::git_merge::conflict_details;
use crate::file::git_write::map_git_error;
use crate::file::util::{resolve_restore_target, validate_workspace_relative_path};
use crate::file::FileCheckpointManager;
use crate::git_store::feat_ref_for_name;
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
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
        // Reuse the pending submission when it already points at the
        // current edit head; otherwise submit a fresh review ref. Edit to
        // review stays a ref copy, never a merge.
        let (review_ref, review_head) = self.reuse_or_submit_review(entity_id)?;

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
            let dominated = match &feature_head {
                Some(feature) => git
                    .is_ancestor(&review_head, feature)
                    .map_err(map_git_error)
                    .unwrap_or(false),
                None => false,
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
                let mut outcome = crate::git_store::merge_file_contents(base, ours, theirs);
                if !outcome.conflicted && outcome.bytes.is_none() && dominated {
                    // A withheld file reads as a feature-side deletion once
                    // its review is an ancestor of the feature. Restore the
                    // add semantics so earlier partial approvals stay
                    // completable.
                    if let Some(theirs_bytes) = theirs {
                        if base == Some(theirs_bytes) {
                            outcome.bytes = Some(theirs_bytes.to_vec());
                        }
                    }
                }
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
            let review_session = review_commit
                .trailer(crate::git_store::TRAILER_SESSION)
                .unwrap_or_default();
            let parent_tree = feature_head
                .as_ref()
                .and_then(|h| git.read_commit(h).ok())
                .map(|c| c.tree);
            let merged_tree = git
                .build_tree_from_parent(parent_tree.as_deref(), &staged)
                .map_err(map_git_error)?;
            if let Some(parent) = parent_tree.as_deref() {
                if parent == merged_tree {
                    let existing = feature_head.clone().unwrap_or(review_head.clone());
                    return Ok(MergeOutcome {
                        merged: false,
                        snapshot_id: existing,
                        conflicts: vec![],
                        conflict_files: vec![],
                        message: "no approved files; changes remain pending".to_string(),
                    });
                }
            }
            let mut parents = Vec::new();
            if let Some(head) = feature_head.clone() {
                parents.push(head);
            }
            parents.push(review_head.clone());
            let mut extra_trailers = Vec::new();
            if !conflict_files.is_empty() {
                extra_trailers.push((
                    crate::git_store::TRAILER_STATE.to_string(),
                    crate::git_store::STATE_CONFLICT_UNRESOLVED.to_string(),
                ));
                let mut sorted_conflicts = conflict_files.clone();
                sorted_conflicts.sort();
                for file in &sorted_conflicts {
                    extra_trailers.push((
                        crate::git_store::TRAILER_CONFLICT_FILE.to_string(),
                        file.clone(),
                    ));
                }
            }
            let message = crate::git_store::commit_message(
                "file-level approval",
                Some(actor.as_str()),
                Some(review_session.as_str()).filter(|s| !s.is_empty()),
                Some("review"),
                &extra_trailers,
            );
            let id = git
                .write_commit(
                    &merged_tree,
                    &parents,
                    actor.as_str(),
                    crate::git_store::SYSTEM_COMMITTER,
                    self.creation_timestamp()?,
                    &message,
                )
                .map_err(map_git_error)?;
            git.compare_and_swap(&feature_ref, feature_head.as_deref(), &id)
                .map_err(map_git_error)?;
            self.index_commit(
                storage,
                &id,
                actor.as_str(),
                &review_session,
                "review",
                &commit_paths,
            )?;
            for path in &commit_paths {
                let bytes = merged_changes.get(path).and_then(|c| c.as_deref());
                self.publish_file_event(&id, path, actor.as_str(), bytes);
            }
            let head = id;
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
                if let Some(ref bus) = self.event_bus {
                    bus.publish(CheckpointEventBus::approval_resolved(
                        actor.as_str().to_string(),
                        ApprovalDecision::Approved,
                        head.clone(),
                    ));
                }
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
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::approval_resolved(
                    actor.as_str().to_string(),
                    ApprovalDecision::Approved,
                    head.clone(),
                ));
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
                if let Some(ref bus) = self.event_bus {
                    bus.publish(CheckpointEventBus::approval_resolved(
                        actor.as_str().to_string(),
                        ApprovalDecision::Approved,
                        merged.commit_id.clone(),
                    ));
                }
                Ok(MergeOutcome {
                    merged: true,
                    snapshot_id: merged.commit_id.clone(),
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

    /// Full merge entry point for an actor: reuse the pending review when
    /// it matches the current edit head, otherwise submit, then merge
    /// into the named feature (the `ApprovalPolicy::auto` path). The merge
    /// commit itself is the multi-parent record linking the feature head
    /// and the review commit.
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
