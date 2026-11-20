//! Git-backed submit/merge/rollback: ref copies and full-ancestry merges
//! on the commit DAG.
//!
//! Semantics enforced here:
//! - edit to review is a ref copy, never a merge;
//! - review to feature and feature to main carry full ancestry: a
//!   multi-parent merge when the target ref already has a head, and a
//!   single-parent fast-forward when the target is still empty;
//! - review state is the explicit `review_state` table (pending / approved
//!   / rejected); rejection deletes the review ref, approval deletes it
//!   after a clean merge while the merge parents preserve history;
//! - conflicts land with standard markers, stay on the target feature ref
//!   as unresolved commits, and block the corresponding main merge until
//!   resolved (no new main commit is created meanwhile);
//! - rolling back one feature creates a new revert commit on main; other
//!   merged features are untouched (main never moves backwards);
//! - an actor's local undo only moves its own edit ref.

use std::collections::HashMap;

use crate::event::CheckpointEventBus;
use crate::file::git_write::map_git_error;
use crate::file::FileCheckpointManager;
use crate::git_store::{
    commit_message, edit_ref_for_actor, feat_ref_for_name, merge_file_maps, review_ref_for_id,
    REF_MAIN, REF_REVIEW_PREFIX, STATE_CONFLICT_UNRESOLVED, TRAILER_CONFLICT_FILE, TRAILER_FEATURE,
    TRAILER_MERGE_ID, TRAILER_STATE,
};
use crate::storage::{JournalEffects, ReviewStatus};
use checkpoint_base::error::CheckpointError;
use wf_types::config::file_checkpoint::ConflictBehavior;

/// One conflict region inside a merged file, parsed back from the diff3
/// markers the merge driver embeds. Line numbers are 0-indexed into the
/// merged output; `end_line` is exclusive.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitConflictRegion {
    pub start_line: usize,
    pub end_line: usize,
    pub base_lines: Vec<String>,
    pub ours_lines: Vec<String>,
    pub theirs_lines: Vec<String>,
}

/// One conflicted file captured during a merge: the true marker regions
/// plus a binary flag. Binary conflicts carry no regions (their bytes hold
/// no markers) and stay file-level only.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitConflictDetail {
    pub file: String,
    pub binary: bool,
    pub regions: Vec<GitConflictRegion>,
}

/// Parse diff3 (`<<<<<<< ours` / `||||||| base` / `=======` /
/// `>>>>>>> theirs`) and legacy two-way markers from merged bytes into
/// regions with their merged-output spans.
pub fn parse_conflict_regions(bytes: &[u8]) -> Vec<GitConflictRegion> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut ours: Vec<String> = Vec::new();
    let mut base: Vec<String> = Vec::new();
    let mut theirs: Vec<String> = Vec::new();
    let mut state = 0u8;
    let mut start_line = 0usize;
    for (idx, line) in text.lines().enumerate() {
        match (state, line) {
            (0, "<<<<<<< ours") => {
                state = 1;
                start_line = idx;
                ours.clear();
                base.clear();
                theirs.clear();
            }
            (1, l) if l.starts_with("|||||||") => state = 3,
            (1, "=======") => state = 2,
            (3, "=======") => state = 2,
            (2, l) if l.starts_with(">>>>>>>") => {
                state = 0;
                out.push(GitConflictRegion {
                    start_line,
                    end_line: idx + 1,
                    base_lines: std::mem::take(&mut base),
                    ours_lines: std::mem::take(&mut ours),
                    theirs_lines: std::mem::take(&mut theirs),
                });
            }
            (1, l) => ours.push(l.to_string()),
            (3, l) => base.push(l.to_string()),
            (2, l) => theirs.push(l.to_string()),
            _ => {}
        }
    }
    if state != 0 {
        out.push(GitConflictRegion {
            start_line,
            end_line: text.lines().count(),
            base_lines: std::mem::take(&mut base),
            ours_lines: std::mem::take(&mut ours),
            theirs_lines: std::mem::take(&mut theirs),
        });
    }
    out
}

/// Build per-file conflict details from a merge outcome: regions come from
/// the merged bytes themselves, so views carry true intervals. Files whose
/// merged bytes hold no markers (binary conflicts) are flagged binary and
/// stay file-level only.
pub fn conflict_details(
    merged: &HashMap<String, Option<Vec<u8>>>,
    conflicts: &[String],
) -> Vec<GitConflictDetail> {
    conflicts
        .iter()
        .map(|file| {
            let regions = merged
                .get(file)
                .and_then(|content| content.as_deref())
                .map(parse_conflict_regions)
                .unwrap_or_default();
            GitConflictDetail {
                file: file.clone(),
                binary: regions.is_empty(),
                regions,
            }
        })
        .collect()
}

/// Side-effect-free three-way merge plan shared by previews and execution.
#[derive(Debug, Clone)]
pub(crate) struct MergePlan {
    pub merged: HashMap<String, Option<Vec<u8>>>,
    pub paths: Vec<String>,
    pub conflicts: Vec<String>,
    pub details: Vec<GitConflictDetail>,
}

/// Compute the merged content, conflicting paths and true marker intervals
/// from three trees. No Git object, ref or SQL row is written.
pub(crate) fn compute_merge_plan(
    base: &HashMap<String, Vec<u8>>,
    ours: &HashMap<String, Vec<u8>>,
    theirs: &HashMap<String, Vec<u8>>,
    restore_dominated_adds: bool,
) -> MergePlan {
    let (mut merged, conflicts) = merge_file_maps(base, ours, theirs);
    if restore_dominated_adds {
        // A withheld file reads as an ours-side deletion once their side is
        // an ancestor. Restore the add so earlier partial approvals stay
        // completable; genuine resolutions keep the existing content.
        for (path, theirs_bytes) in theirs {
            if ours.contains_key(path) {
                continue;
            }
            if merged.get(path).is_some_and(|value| value.is_some()) {
                continue;
            }
            if base.get(path) != Some(theirs_bytes) {
                continue;
            }
            merged.insert(path.clone(), Some(theirs_bytes.clone()));
        }
    }
    let details = conflict_details(&merged, &conflicts);
    let mut paths: Vec<String> = merged.keys().cloned().collect();
    paths.sort();
    MergePlan {
        merged,
        paths,
        conflicts,
        details,
    }
}

/// How one detected conflict set must be handled. This is the single policy
/// decision point for every merge entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergeConflictDecision {
    Clean,
    Marker,
    Fail,
    Defer,
}

pub(crate) fn decide_merge_conflict(
    behavior: ConflictBehavior,
    conflicts: &[String],
) -> MergeConflictDecision {
    if conflicts.is_empty() {
        MergeConflictDecision::Clean
    } else {
        match behavior {
            ConflictBehavior::Marker => MergeConflictDecision::Marker,
            ConflictBehavior::Fail => MergeConflictDecision::Fail,
            ConflictBehavior::Approval => MergeConflictDecision::Defer,
        }
    }
}

/// Structured result of a strategy-aware merge. `merged = false` means no
/// ref moved and no merge commit was created; conflicts are still reported.
#[derive(Debug, Clone)]
pub(crate) struct MergeExecutionOutcome {
    pub merged: bool,
    pub outcome: GitMergeOutcome,
}

impl MergeExecutionOutcome {
    fn deferred(head: String, parents: Vec<String>, plan: &MergePlan) -> Self {
        Self {
            merged: false,
            outcome: GitMergeOutcome {
                commit_id: head,
                parents,
                conflict_files: plan.conflicts.clone(),
                details: plan.details.clone(),
            },
        }
    }
}

/// Outcome of a Git merge: the merge commit plus per-file conflicts.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitMergeOutcome {
    pub commit_id: String,
    pub parents: Vec<String>,
    pub conflict_files: Vec<String>,
    pub details: Vec<GitConflictDetail>,
}

impl GitMergeOutcome {
    pub fn has_conflicts(&self) -> bool {
        !self.conflict_files.is_empty()
    }

    pub fn conflict_count(&self) -> usize {
        self.conflict_files.len()
    }
}

/// Authoritative conflict status of one ref head. The `Wf-State` /
/// `Wf-Conflict-File` trailers are the single source of truth; conflict
/// markers embedded in file bytes are display-only and never decide
/// blocking. All merge gates and health listings read through this
/// helper so blocking and enumeration cannot drift apart.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConflictStatus {
    /// Whether the ref head carries an unresolved merge.
    pub unresolved: bool,
    /// Conflict files recorded in the head commit trailers.
    pub files: Vec<String>,
}

impl ConflictStatus {
    pub fn clean() -> Self {
        Self {
            unresolved: false,
            files: Vec::new(),
        }
    }
}

impl FileCheckpointManager {
    /// Authoritative conflict status of one ref: reads the head commit
    /// trailers (`Wf-State` / `Wf-Conflict-File`). A missing ref or an
    /// unreadable head reports clean so cleanup favors safety; merge
    /// gates treat a missing feature as not-found separately.
    pub fn conflict_status_for_ref(
        &self,
        refname: &str,
    ) -> Result<ConflictStatus, CheckpointError> {
        let git = self.git_ref()?;
        let head = git.read_ref(refname).map_err(map_git_error)?;
        let Some(head) = head else {
            return Ok(ConflictStatus::clean());
        };
        let commit = git.read_commit(&head).map_err(map_git_error)?;
        if commit.trailer(TRAILER_STATE).as_deref() == Some(STATE_CONFLICT_UNRESOLVED) {
            Ok(ConflictStatus {
                unresolved: true,
                files: commit.trailers(TRAILER_CONFLICT_FILE),
            })
        } else {
            Ok(ConflictStatus::clean())
        }
    }

    /// Submit an actor's edit line for review: copy the edit ref to a fresh
    /// review ref (no merge) and mark it pending. Returns
    /// `(review ref, commit id)`.
    pub(crate) fn submit_for_review(
        &self,
        entity_id: &str,
    ) -> Result<(String, String), CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let git = self.git_ref()?;
        let edit_ref = edit_ref_for_actor(actor.as_str());
        let head = git
            .read_ref(&edit_ref)
            .map_err(map_git_error)?
            .ok_or_else(|| CheckpointError::Validation {
                reason: format!("actor '{}' has no commits to submit", actor.as_str()),
            })?;
        let timestamp = self.creation_timestamp()?;
        let head_prefix = head.get(..8).unwrap_or(&head).to_string();
        for attempt in 0..8u32 {
            let review_id = format!(
                "{}-{}-{}-{}",
                sanitize(actor.as_str()),
                timestamp,
                head_prefix,
                attempt
            );
            let review_ref = review_ref_for_id(&review_id);
            let spec = crate::file::journal::RefUpdateSpec {
                kind: crate::storage::RefOperationKind::ReviewSubmit,
                reason: "submit actor edits for review".to_string(),
                actor: actor.as_str().to_string(),
                session: String::new(),
                tool: "review".to_string(),
                ref_name: review_ref.clone(),
                old_id: None,
                new_id: Some(head.clone()),
                related_refs: vec![edit_ref.clone()],
                effects: crate::file::journal::review_effects(
                    &review_ref,
                    Some(ReviewStatus::Pending),
                ),
            };
            match self
                .journaled_ref_update(spec, |_| git.compare_and_swap(&review_ref, None, &head))
            {
                Ok(()) => {
                    if let Some(ref bus) = self.event_bus {
                        bus.publish(CheckpointEventBus::review_submitted(
                            actor.as_str().to_string(),
                            head.clone(),
                        ));
                    }
                    return Ok((review_ref, head));
                }
                Err(CheckpointError::Branch(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(CheckpointError::Branch(
            "concurrent review submission conflict".to_string(),
        ))
    }

    /// Newest pending review ref for an actor, if any.
    pub(crate) fn newest_pending_review(
        &self,
        actor_str: &str,
    ) -> Result<Option<(String, String)>, CheckpointError> {
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut pending: Vec<(String, String, i64)> = Vec::new();
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
            if commit.trailer(crate::git_store::TRAILER_ACTOR).as_deref() != Some(actor_str) {
                continue;
            }
            pending.push((review_ref, head, commit.committer_ts));
        }
        pending.sort_by_key(|item| std::cmp::Reverse(item.2));
        Ok(pending.into_iter().next().map(|(r, h, _)| (r, h)))
    }

    /// Reuse the newest pending review when it already points at the
    /// current edit head; otherwise submit a fresh review ref. This keeps
    /// repeated approvals without new edits from piling up review refs
    /// while still capturing edits that advanced since the last submit.
    pub(crate) fn reuse_or_submit_review(
        &self,
        entity_id: &str,
    ) -> Result<(String, String), CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        if let Some((review_ref, review_head)) = self.newest_pending_review(actor.as_str())? {
            let git = self.git_ref()?;
            let edit_head = git
                .read_ref(&edit_ref_for_actor(actor.as_str()))
                .map_err(map_git_error)?;
            match edit_head {
                None => return Ok((review_ref, review_head)),
                Some(edit) if edit == review_head => return Ok((review_ref, review_head)),
                _ => {}
            }
        }
        self.submit_for_review(entity_id)
    }

    /// Merge a review head into a feature using the manager's configured
    /// conflict behavior.
    pub(crate) fn merge_review_into_feature(
        &self,
        review_ref: &str,
        review_head: &str,
        feature_name: &str,
        actor_str: &str,
    ) -> Result<MergeExecutionOutcome, CheckpointError> {
        self.merge_review_into_feature_with_behavior(
            review_ref,
            review_head,
            feature_name,
            actor_str,
            self.policy.conflict_behavior,
        )
    }

    /// Strategy-aware review-to-feature merge: conflicts are precomputed,
    /// then one decision decides between a conflict commit, an error or a
    /// deferred (unmerged) result before any ref is touched.
    pub(crate) fn merge_review_into_feature_with_behavior(
        &self,
        review_ref: &str,
        review_head: &str,
        feature_name: &str,
        actor_str: &str,
        behavior: ConflictBehavior,
    ) -> Result<MergeExecutionOutcome, CheckpointError> {
        crate::branch::ensure_feature_branch_name(feature_name)?;
        let git = self.git_ref()?;
        let feature_ref = feat_ref_for_name(feature_name);
        let feature_head = git.read_ref(&feature_ref).map_err(map_git_error)?;
        let status = self.conflict_status_for_ref(&feature_ref)?;
        if status.unresolved {
            return Err(CheckpointError::MergeConflict {
                actor: actor_str.to_string(),
                files: status.files,
            });
        }
        let review_commit = git.read_commit(review_head).map_err(map_git_error)?;
        let review_tree = git
            .tree_to_bytes(&review_commit.tree)
            .map_err(map_git_error)?;
        let feature_tree: HashMap<String, Vec<u8>> = match &feature_head {
            Some(head) => {
                let commit = git.read_commit(head).map_err(map_git_error)?;
                git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
            }
            None => HashMap::new(),
        };
        let base_tree: HashMap<String, Vec<u8>> = match &feature_head {
            Some(feature) => match git
                .merge_base(feature, review_head)
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
                .is_ancestor(review_head, feature)
                .map_err(map_git_error)
                .unwrap_or(false),
            None => false,
        };
        let plan = compute_merge_plan(&base_tree, &feature_tree, &review_tree, dominated);
        let mut parents = Vec::new();
        if let Some(head) = feature_head.clone() {
            parents.push(head);
        }
        parents.push(review_head.to_string());
        match decide_merge_conflict(behavior, &plan.conflicts) {
            MergeConflictDecision::Fail => {
                let mut files = plan.conflicts.clone();
                files.sort();
                return Err(CheckpointError::MergeConflict {
                    actor: actor_str.to_string(),
                    files,
                });
            }
            MergeConflictDecision::Defer => {
                return Ok(MergeExecutionOutcome::deferred(
                    feature_head
                        .clone()
                        .unwrap_or_else(|| review_head.to_string()),
                    parents,
                    &plan,
                ));
            }
            MergeConflictDecision::Clean | MergeConflictDecision::Marker => {}
        }
        let mut changes: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
        for (path, content) in &plan.merged {
            changes.insert(
                path.clone(),
                content
                    .clone()
                    .map(|bytes| (crate::git_store::MODE_FILE.to_string(), bytes)),
            );
        }
        let parent_tree = feature_head
            .as_ref()
            .and_then(|h| git.read_commit(h).ok())
            .map(|c| c.tree);
        let merged_tree = git
            .build_tree_from_parent(parent_tree.as_deref(), &changes)
            .map_err(map_git_error)?;
        let review_session = review_commit
            .trailer(crate::git_store::TRAILER_SESSION)
            .unwrap_or_default();
        let has_conflicts = !plan.conflicts.is_empty();
        let mut trailers = Vec::new();
        if has_conflicts {
            trailers.push((
                TRAILER_STATE.to_string(),
                STATE_CONFLICT_UNRESOLVED.to_string(),
            ));
            let mut sorted = plan.conflicts.clone();
            sorted.sort();
            for file in &sorted {
                trailers.push((TRAILER_CONFLICT_FILE.to_string(), file.clone()));
            }
        }
        let suffix = if has_conflicts { " (conflicted)" } else { "" };
        let message = commit_message(
            &format!("merge review into feature '{feature_name}'{suffix}"),
            Some(actor_str),
            Some(review_session.as_str()).filter(|s| !s.is_empty()),
            Some("review"),
            &trailers,
        );
        if !has_conflicts && parent_tree.as_deref() == Some(merged_tree.as_str()) {
            let effects =
                crate::file::journal::review_effects(review_ref, Some(ReviewStatus::Approved));
            let spec = crate::file::journal::RefUpdateSpec {
                kind: crate::storage::RefOperationKind::ReviewApprove,
                reason: format!("approve review into feature '{feature_name}'"),
                actor: actor_str.to_string(),
                session: review_session.clone(),
                tool: "review".to_string(),
                ref_name: review_ref.to_string(),
                old_id: git.read_ref(review_ref).map_err(map_git_error)?,
                new_id: None,
                related_refs: vec![feature_ref.clone()],
                effects: JournalEffects {
                    deleted_review_refs: vec![review_ref.to_string()],
                    ..effects
                },
            };
            self.journaled_ref_update(spec, |_| git.delete_ref(review_ref))?;
            return Ok(MergeExecutionOutcome {
                merged: true,
                outcome: GitMergeOutcome {
                    commit_id: feature_head.clone().unwrap_or_default(),
                    parents,
                    conflict_files: Vec::new(),
                    details: Vec::new(),
                },
            });
        }
        let id = git
            .write_commit(
                &merged_tree,
                &parents,
                actor_str,
                crate::git_store::SYSTEM_COMMITTER,
                self.creation_timestamp()?,
                &message,
            )
            .map_err(map_git_error)?;
        let status = if has_conflicts {
            Some(ReviewStatus::Pending)
        } else {
            Some(ReviewStatus::Approved)
        };
        let mut effects = crate::file::journal::review_effects(review_ref, status);
        if !has_conflicts {
            effects.deleted_review_refs.push(review_ref.to_string());
        }
        effects.source_index = Some(crate::storage::SourceIndexEntry {
            commit_id: id.clone(),
            actor: actor_str.to_string(),
            session: review_session.clone(),
            tool: "review".to_string(),
            paths: plan.paths.clone(),
            timestamp: git.read_commit(&id).map_err(map_git_error)?.committer_ts,
        });
        let spec = crate::file::journal::RefUpdateSpec {
            kind: if has_conflicts {
                crate::storage::RefOperationKind::ReviewMerge
            } else {
                crate::storage::RefOperationKind::ReviewApprove
            },
            reason: format!("merge review into feature '{feature_name}'"),
            actor: actor_str.to_string(),
            session: review_session.clone(),
            tool: "review".to_string(),
            ref_name: feature_ref.clone(),
            old_id: feature_head.clone(),
            new_id: Some(id.clone()),
            related_refs: vec![review_ref.to_string()],
            effects,
        };
        self.journaled_ref_update(spec, |update| {
            git.compare_and_swap(
                &update.ref_name,
                update.old_id.as_deref(),
                update.new_id.as_deref().unwrap_or_default(),
            )
        })?;
        if has_conflicts {
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::merge_conflicted(
                    id.clone(),
                    plan.conflicts.clone(),
                    Some(actor_str.to_string()),
                ));
            }
            if let Some(root) = self.workspace_root.clone() {
                let _ = self.materialize_files_into(&plan.merged, &plan.conflicts, &root);
            }
        }
        Ok(MergeExecutionOutcome {
            merged: true,
            outcome: GitMergeOutcome {
                commit_id: id,
                parents,
                conflict_files: plan.conflicts,
                details: plan.details,
            },
        })
    }

    /// Merge one feature ref into main (single sequential step; callers
    /// iterate for multiple features so each keeps its own merge commit).
    /// Single policy decision point for feature-to-main merges. Conflicts
    /// are predicted from the same plan used by the dry-run preview.
    pub(crate) fn merge_feature_into_main_with_behavior(
        &self,
        feature_name: &str,
        actor_str: &str,
        behavior: ConflictBehavior,
    ) -> Result<MergeExecutionOutcome, CheckpointError> {
        crate::branch::ensure_feature_branch_name(feature_name)?;
        let git = self.git_ref()?;
        let feature_ref = feat_ref_for_name(feature_name);
        let feature_head = git
            .read_ref(&feature_ref)
            .map_err(map_git_error)?
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("feature '{feature_name}'"),
            })?;
        let feature_commit = git.read_commit(&feature_head).map_err(map_git_error)?;
        let feature_status = self.conflict_status_for_ref(&feature_ref)?;
        if feature_status.unresolved {
            return Err(CheckpointError::MergeConflict {
                actor: actor_str.to_string(),
                files: feature_status.files,
            });
        }
        let main_head = git.read_ref(REF_MAIN).map_err(map_git_error)?;
        let main_status = self.conflict_status_for_ref(REF_MAIN)?;
        if main_status.unresolved {
            return Err(CheckpointError::MergeConflict {
                actor: actor_str.to_string(),
                files: main_status.files,
            });
        }
        let feature_tree = git
            .tree_to_bytes(&feature_commit.tree)
            .map_err(map_git_error)?;
        let main_tree: HashMap<String, Vec<u8>> = match &main_head {
            Some(head) => {
                let commit = git.read_commit(head).map_err(map_git_error)?;
                git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
            }
            None => HashMap::new(),
        };
        let base_tree: HashMap<String, Vec<u8>> = match &main_head {
            Some(main) => match git.merge_base(main, &feature_head).map_err(map_git_error)? {
                Some(base) => {
                    let commit = git.read_commit(&base).map_err(map_git_error)?;
                    git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
                }
                None => HashMap::new(),
            },
            None => HashMap::new(),
        };
        let plan = compute_merge_plan(&base_tree, &main_tree, &feature_tree, false);
        let mut feature_parents = vec![feature_head.clone()];
        if let Some(main) = main_head.clone() {
            if main != feature_head {
                feature_parents.push(main);
            }
        }
        match decide_merge_conflict(behavior, &plan.conflicts) {
            MergeConflictDecision::Fail => {
                let mut files = plan.conflicts.clone();
                files.sort();
                return Err(CheckpointError::MergeConflict {
                    actor: actor_str.to_string(),
                    files,
                });
            }
            MergeConflictDecision::Defer => {
                return Ok(MergeExecutionOutcome::deferred(
                    feature_head.clone(),
                    feature_parents,
                    &plan,
                ));
            }
            MergeConflictDecision::Clean | MergeConflictDecision::Marker => {}
        }
        let has_conflicts = !plan.conflicts.is_empty();
        let mut changes: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
        for (path, content) in &plan.merged {
            changes.insert(
                path.clone(),
                content
                    .clone()
                    .map(|bytes| (crate::git_store::MODE_FILE.to_string(), bytes)),
            );
        }
        let target_ref = if has_conflicts {
            &feature_ref
        } else {
            REF_MAIN
        };
        let target_head = if has_conflicts {
            Some(feature_head.clone())
        } else {
            main_head.clone()
        };
        let parent_tree = if has_conflicts {
            Some(feature_commit.tree.clone())
        } else {
            main_head
                .as_ref()
                .and_then(|h| git.read_commit(h).ok())
                .map(|c| c.tree)
        };
        let merged_tree = git
            .build_tree_from_parent(parent_tree.as_deref(), &changes)
            .map_err(map_git_error)?;
        if !has_conflicts && parent_tree.as_deref() == Some(merged_tree.as_str()) {
            let parents = main_head
                .iter()
                .cloned()
                .chain(std::iter::once(feature_head.clone()))
                .collect();
            return Ok(MergeExecutionOutcome {
                merged: true,
                outcome: GitMergeOutcome {
                    commit_id: main_head.clone().unwrap_or_default(),
                    parents,
                    conflict_files: Vec::new(),
                    details: Vec::new(),
                },
            });
        }
        let mut trailers = Vec::new();
        if has_conflicts {
            trailers.push((
                TRAILER_STATE.to_string(),
                STATE_CONFLICT_UNRESOLVED.to_string(),
            ));
            let mut sorted = plan.conflicts.clone();
            sorted.sort();
            for file in &sorted {
                trailers.push((TRAILER_CONFLICT_FILE.to_string(), file.clone()));
            }
        } else {
            let merge_ts = self.creation_timestamp()?;
            let head_prefix = feature_head.get(..8).unwrap_or(&feature_head).to_string();
            trailers.push((TRAILER_FEATURE.to_string(), feature_name.to_string()));
            trailers.push((
                TRAILER_MERGE_ID.to_string(),
                format!("{feature_name}-{merge_ts}-{head_prefix}"),
            ));
        }
        let suffix = if has_conflicts { " (conflicted)" } else { "" };
        let message = commit_message(
            &format!("merge feature '{feature_name}' into main{suffix}"),
            Some(actor_str),
            None,
            Some("merge"),
            &trailers,
        );
        let commit_parents: Vec<String> = if has_conflicts {
            feature_parents.clone()
        } else {
            main_head
                .iter()
                .cloned()
                .chain(std::iter::once(feature_head.clone()))
                .collect()
        };
        let id = git
            .write_commit(
                &merged_tree,
                &commit_parents,
                actor_str,
                crate::git_store::SYSTEM_COMMITTER,
                self.creation_timestamp()?,
                &message,
            )
            .map_err(map_git_error)?;
        let mut effects = crate::file::journal::index_effects(crate::storage::SourceIndexEntry {
            commit_id: id.clone(),
            actor: actor_str.to_string(),
            session: String::new(),
            tool: "merge".to_string(),
            paths: plan.paths.clone(),
            timestamp: git.read_commit(&id).map_err(map_git_error)?.committer_ts,
        });
        if has_conflicts {
            effects.review_states = Vec::new();
        }
        let spec = crate::file::journal::RefUpdateSpec {
            kind: crate::storage::RefOperationKind::FeatureMerge,
            reason: format!("merge feature '{feature_name}' into main"),
            actor: actor_str.to_string(),
            session: String::new(),
            tool: "merge".to_string(),
            ref_name: target_ref.to_string(),
            old_id: target_head.clone(),
            new_id: Some(id.clone()),
            related_refs: if has_conflicts {
                vec![REF_MAIN.to_string()]
            } else {
                vec![feature_ref.clone()]
            },
            effects,
        };
        self.journaled_ref_update(spec, |update| {
            git.compare_and_swap(
                &update.ref_name,
                update.old_id.as_deref(),
                update.new_id.as_deref().unwrap_or_default(),
            )
        })?;
        if has_conflicts {
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::merge_conflicted(
                    id.clone(),
                    plan.conflicts.clone(),
                    Some(actor_str.to_string()),
                ));
            }
            if let Some(root) = self.workspace_root.clone() {
                let _ = self.materialize_files_into(&plan.merged, &plan.conflicts, &root);
            }
        }
        Ok(MergeExecutionOutcome {
            merged: true,
            outcome: GitMergeOutcome {
                commit_id: id,
                parents: commit_parents,
                conflict_files: plan.conflicts,
                details: plan.details,
            },
        })
    }

    /// Roll back one feature from main by appending a revert commit that
    /// backs out exactly what the feature's merge brought in. The reference
    /// is the main merge that absorbed the feature (its first parent is
    /// the pre-merge main); other merged features are untouched and main
    /// never moves backwards.
    pub(crate) fn revert_feature_on_main(
        &self,
        feature_head: &str,
        actor_str: &str,
    ) -> Result<GitMergeOutcome, CheckpointError> {
        let git = self.git_ref()?;
        let main_head = git
            .read_ref(REF_MAIN)
            .map_err(map_git_error)?
            .ok_or_else(|| CheckpointError::Validation {
                reason: "main has no commits to roll back".to_string(),
            })?;
        if !git
            .is_ancestor(feature_head, &main_head)
            .map_err(map_git_error)?
        {
            return Err(CheckpointError::Validation {
                reason: "feature commit is not merged into main".to_string(),
            });
        }
        // The main merge that absorbed the feature (newest first).
        let absorbing = git
            .log(&main_head, 0)
            .map_err(map_git_error)?
            .into_iter()
            .find(|c| c.parents.iter().any(|p| p == feature_head))
            .ok_or_else(|| CheckpointError::Validation {
                reason: "feature commit is not merged into main".to_string(),
            })?;
        // Pre-merge main: the absorbing merge's first parent (empty when
        // the feature was main's first commit).
        let pre_merge_tree: HashMap<String, Vec<u8>> = match absorbing.parents.first() {
            Some(parent) if parent != feature_head => {
                let commit = git.read_commit(parent).map_err(map_git_error)?;
                git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
            }
            _ => match absorbing.parents.iter().find(|p| *p != feature_head) {
                Some(parent) => {
                    let commit = git.read_commit(parent).map_err(map_git_error)?;
                    git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
                }
                None => HashMap::new(),
            },
        };
        let main_commit = git.read_commit(&main_head).map_err(map_git_error)?;
        let main_tree = git
            .tree_to_bytes(&main_commit.tree)
            .map_err(map_git_error)?;
        let feature_commit = git.read_commit(feature_head).map_err(map_git_error)?;
        let feature_tree = git
            .tree_to_bytes(&feature_commit.tree)
            .map_err(map_git_error)?;
        // Inverse application: base = the feature result as merged,
        // theirs = main just before the absorbing merge.
        let (merged, conflicts) = merge_file_maps(&feature_tree, &main_tree, &pre_merge_tree);
        if !conflicts.is_empty() {
            let mut sorted = conflicts.clone();
            sorted.sort();
            return Err(CheckpointError::MergeConflict {
                actor: actor_str.to_string(),
                files: sorted,
            });
        }
        let mut changes: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
        let mut all_paths: Vec<String> = Vec::new();
        for (path, content) in &merged {
            all_paths.push(path.clone());
            changes.insert(
                path.clone(),
                content
                    .clone()
                    .map(|bytes| (crate::git_store::MODE_FILE.to_string(), bytes)),
            );
        }
        all_paths.sort();
        let merged_tree = git
            .build_tree_from_parent(Some(&main_commit.tree), &changes)
            .map_err(map_git_error)?;
        if merged_tree == main_commit.tree {
            return Ok(GitMergeOutcome {
                commit_id: main_head.clone(),
                parents: vec![main_head.clone()],
                conflict_files: Vec::new(),
                details: Vec::new(),
            });
        }
        let parents = vec![main_head.clone()];
        let message = commit_message(
            &format!("revert feature merge {feature_head}"),
            Some(actor_str),
            None,
            Some("rollback"),
            &[],
        );
        let id = git
            .write_commit(
                &merged_tree,
                &parents,
                actor_str,
                crate::git_store::SYSTEM_COMMITTER,
                self.creation_timestamp()?,
                &message,
            )
            .map_err(map_git_error)?;
        let spec = crate::file::journal::RefUpdateSpec {
            kind: crate::storage::RefOperationKind::FeatureRevert,
            reason: format!("revert feature merge {feature_head}"),
            actor: actor_str.to_string(),
            session: String::new(),
            tool: "rollback".to_string(),
            ref_name: REF_MAIN.to_string(),
            old_id: Some(main_head.clone()),
            new_id: Some(id.clone()),
            related_refs: vec![feature_head.to_string()],
            effects: crate::file::journal::index_effects(crate::storage::SourceIndexEntry {
                commit_id: id.clone(),
                actor: actor_str.to_string(),
                session: String::new(),
                tool: "rollback".to_string(),
                paths: all_paths.clone(),
                timestamp: self.creation_timestamp()?,
            }),
        };
        self.journaled_ref_update(spec, |update| {
            git.compare_and_swap(
                &update.ref_name,
                update.old_id.as_deref(),
                update.new_id.as_deref().unwrap_or_default(),
            )
        })?;
        let details = conflict_details(&merged, &conflicts);
        Ok(GitMergeOutcome {
            commit_id: id.clone(),
            parents,
            conflict_files: conflicts,
            details,
        })
    }

    /// Write already-merged bytes for exactly the conflicted paths into the
    /// worktree (standard markers on disk). Non-conflicted paths are left
    /// alone; protected ignore names are never written.
    pub(crate) fn materialize_files_into(
        &self,
        merged: &HashMap<String, Option<Vec<u8>>>,
        conflicts: &[String],
        root: &std::path::Path,
    ) -> Result<(), CheckpointError> {
        for file in conflicts {
            let Some(Some(bytes)) = merged.get(file) else {
                continue;
            };
            if crate::scan::is_hardcoded_ignored(file) {
                continue;
            }
            let relative = crate::file::util::validate_workspace_relative_path(file)?;
            let target = root.join(relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, bytes)?;
        }
        Ok(())
    }
}

/// Submission-id shaping reuses the single ref sanitizer so generated
/// review ids always survive ref construction unchanged.
fn sanitize(raw: &str) -> String {
    crate::git_store::sanitize_ref_component(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::FileContentEntry;
    use crate::git_store::feat_ref_for_name;

    fn diff3_bytes(ours: &str, base: &str, theirs: &str) -> Vec<u8> {
        format!("<<<<<<< ours\n{ours}||||||| base\n{base}=======\n{theirs}>>>>>>> theirs\n")
            .into_bytes()
    }

    fn manager() -> FileCheckpointManager {
        FileCheckpointManager::new_in_memory().expect("in-memory manager")
    }

    #[test]
    fn parse_diff3_markers_recovers_true_spans() {
        let regions =
            parse_conflict_regions(&diff3_bytes("our line\n", "base line\n", "their line\n"));
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start_line, 0);
        assert_eq!(regions[0].end_line, 7);
        assert_eq!(regions[0].ours_lines, vec!["our line".to_string()]);
        assert_eq!(regions[0].base_lines, vec!["base line".to_string()]);
        assert_eq!(regions[0].theirs_lines, vec!["their line".to_string()]);
    }

    #[test]
    fn parse_legacy_two_way_markers_leave_base_empty() {
        let bytes = b"<<<<<<< ours\nour\n=======\ntheir\n>>>>>>> theirs\n".to_vec();
        let regions = parse_conflict_regions(&bytes);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].ours_lines, vec!["our".to_string()]);
        assert!(regions[0].base_lines.is_empty());
        assert_eq!(regions[0].theirs_lines, vec!["their".to_string()]);
    }

    #[test]
    fn parse_clean_bytes_yield_no_regions() {
        assert!(parse_conflict_regions(b"plain\ntext\n").is_empty());
        assert!(parse_conflict_regions(&[0u8, 1, 2]).is_empty());
    }

    #[test]
    fn parse_unterminated_marker_covers_trailing_lines() {
        let regions = parse_conflict_regions(b"<<<<<<< ours\npartial\n");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].ours_lines, vec!["partial".to_string()]);
        assert_eq!(regions[0].end_line, 2);
    }

    #[test]
    fn parse_multiple_regions_reports_true_merged_line_numbers() {
        let bytes = b"head\n<<<<<<< ours\nA\n=======\nB\n>>>>>>> theirs\nmiddle\n<<<<<<< ours\nC\n=======\nD\n>>>>>>> theirs\ntail\n";
        let regions = parse_conflict_regions(bytes);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].start_line, 1);
        assert_eq!(regions[0].ours_lines, vec!["A".to_string()]);
        assert_eq!(regions[1].start_line, 7);
        assert_eq!(regions[1].theirs_lines, vec!["D".to_string()]);
    }

    #[test]
    fn conflict_details_flags_binary_without_regions() {
        let mut merged: HashMap<String, Option<Vec<u8>>> = HashMap::new();
        merged.insert("a.txt".to_string(), Some(diff3_bytes("o\n", "b\n", "t\n")));
        merged.insert("bin.dat".to_string(), Some(vec![0u8, 1, 2]));
        let details = conflict_details(&merged, &["a.txt".to_string(), "bin.dat".to_string()]);
        let text = details
            .iter()
            .find(|detail| detail.file == "a.txt")
            .expect("a.txt");
        assert!(!text.binary);
        assert_eq!(text.regions.len(), 1);
        let binary = details
            .iter()
            .find(|detail| detail.file == "bin.dat")
            .expect("bin.dat");
        assert!(binary.binary, "marker-less conflicts stay file-level only");
        assert!(binary.regions.is_empty());
    }

    #[test]
    fn fail_behavior_prevents_conflict_commit_before_feature_moves() {
        let mut manager = manager();
        manager.policy.conflict_behavior = ConflictBehavior::Fail;
        manager
            .create_checkpoint(
                "exec-1",
                &[FileContentEntry::new("a.txt", b"base\nline2\n".to_vec())],
            )
            .unwrap();
        manager
            .create_checkpoint(
                "exec-1",
                &[FileContentEntry::new("a.txt", b"one\nline2\n".to_vec())],
            )
            .unwrap();
        manager.merge_entity_changes("exec-1", "feature-1").unwrap();
        let feature_ref = feat_ref_for_name("feature-1");
        let before = manager.git_ref().unwrap().read_ref(&feature_ref).unwrap();

        manager
            .create_checkpoint(
                "exec-2",
                &[FileContentEntry::new("a.txt", b"base\nline2\n".to_vec())],
            )
            .unwrap();
        manager
            .create_checkpoint(
                "exec-2",
                &[FileContentEntry::new("a.txt", b"two\nline2\n".to_vec())],
            )
            .unwrap();
        let err = manager
            .merge_entity_changes("exec-2", "feature-1")
            .unwrap_err();
        assert!(matches!(err, CheckpointError::MergeConflict { .. }));
        assert_eq!(
            manager.git_ref().unwrap().read_ref(&feature_ref).unwrap(),
            before
        );
        assert!(
            !manager
                .conflict_status_for_ref(&feature_ref)
                .unwrap()
                .unresolved
        );
    }

    #[test]
    fn approval_behavior_keeps_review_pending_without_feature_commit() {
        let mut manager = manager();
        manager.policy.conflict_behavior = ConflictBehavior::Approval;
        manager
            .create_checkpoint(
                "exec-1",
                &[FileContentEntry::new("a.txt", b"base\nline2\n".to_vec())],
            )
            .unwrap();
        manager
            .create_checkpoint(
                "exec-1",
                &[FileContentEntry::new("a.txt", b"one\nline2\n".to_vec())],
            )
            .unwrap();
        manager.merge_entity_changes("exec-1", "feature-1").unwrap();
        let feature_ref = feat_ref_for_name("feature-1");
        let before = manager.git_ref().unwrap().read_ref(&feature_ref).unwrap();

        manager
            .create_checkpoint(
                "exec-2",
                &[FileContentEntry::new("a.txt", b"base\nline2\n".to_vec())],
            )
            .unwrap();
        manager
            .create_checkpoint(
                "exec-2",
                &[FileContentEntry::new("a.txt", b"two\nline2\n".to_vec())],
            )
            .unwrap();
        let outcome = manager.merge_entity_changes("exec-2", "feature-1").unwrap();
        assert!(
            !outcome.merge_result.has_conflicts()
                || outcome.merge_result.commit_id == before.clone().unwrap_or_default()
        );
        assert_eq!(
            manager.git_ref().unwrap().read_ref(&feature_ref).unwrap(),
            before
        );
        let actor = manager.actor_id_for("exec-2");
        assert!(manager
            .newest_pending_review(actor.as_str())
            .unwrap()
            .is_some());
    }

    #[test]
    fn conflict_status_tracks_unresolved_feature() {
        let manager = manager();
        let feature_ref = feat_ref_for_name("feature-1");
        assert!(
            !manager
                .conflict_status_for_ref(&feature_ref)
                .unwrap()
                .unresolved,
            "missing ref reports clean so cleanup favors safety"
        );
        assert!(
            !manager
                .conflict_status_for_ref("refs/wf/feat/missing")
                .unwrap()
                .unresolved
        );

        manager
            .create_checkpoint(
                "exec-1",
                &[FileContentEntry::new("a.txt", b"base\nline2\n".to_vec())],
            )
            .unwrap();
        manager
            .create_checkpoint(
                "exec-1",
                &[FileContentEntry::new("a.txt", b"one\nline2\n".to_vec())],
            )
            .unwrap();
        manager.merge_entity_changes("exec-1", "feature-1").unwrap();
        manager
            .create_checkpoint(
                "exec-2",
                &[FileContentEntry::new("a.txt", b"base\nline2\n".to_vec())],
            )
            .unwrap();
        manager
            .create_checkpoint(
                "exec-2",
                &[FileContentEntry::new("a.txt", b"two\nline2\n".to_vec())],
            )
            .unwrap();
        let outcome = manager
            .approve_changes("exec-2", "feature-1", None, ConflictBehavior::Marker, None)
            .unwrap();
        assert!(outcome.has_conflicts());

        let status = manager.conflict_status_for_ref(&feature_ref).unwrap();
        assert!(status.unresolved);
        assert_eq!(status.files, vec!["a.txt".to_string()]);
    }
}
