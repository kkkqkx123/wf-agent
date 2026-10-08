//! Git-backed submit/merge/rollback: ref copies and true multi-parent
//! merges on the commit DAG.
//!
//! Semantics enforced here:
//! - edit to review is a ref copy, never a merge;
//! - review to feature and feature to main are true merges with full
//!   ancestry (multi-parent commits);
//! - review state is the explicit `review_state` table (pending / approved
//!   / rejected); rejection only deletes the review ref;
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
    REF_MAIN, REF_REVIEW_PREFIX, STATE_CONFLICT_UNRESOLVED, TRAILER_CONFLICT_FILE, TRAILER_STATE,
};
use crate::storage::ReviewStatus;
use checkpoint_base::error::CheckpointError;

/// One file-level conflict captured during a merge.
#[derive(Debug, Clone)]
pub struct GitConflictDetail {
    pub file: String,
    pub ours_lines: Vec<String>,
    pub theirs_lines: Vec<String>,
}

/// Outcome of a Git merge: the merge commit plus per-file conflicts.
#[derive(Debug, Clone)]
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

fn split_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::to_string)
        .collect()
}

impl FileCheckpointManager {
    /// Submit an actor's edit line for review: copy the edit ref to a fresh
    /// review ref (no merge) and mark it pending. Returns
    /// `(review ref, commit id)`.
    pub(crate) fn submit_for_review(
        &self,
        entity_id: &str,
    ) -> Result<(String, String), CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let edit_ref = edit_ref_for_actor(actor.as_str());
        let head = git
            .read_ref(&edit_ref)
            .map_err(map_git_error)?
            .ok_or_else(|| CheckpointError::Validation {
                reason: format!("actor '{}' has no commits to submit", actor.as_str()),
            })?;
        let timestamp = self.creation_timestamp().unwrap_or_else(|_| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        });
        let taken = git
            .list_refs(&format!("{REF_REVIEW_PREFIX}{}", sanitize(actor.as_str())))
            .map_err(map_git_error)?
            .len();
        let review_id = format!("{}-{}-{}", sanitize(actor.as_str()), timestamp, taken);
        let review_ref = review_ref_for_id(&review_id);
        git.copy_ref(&edit_ref, &review_ref)
            .map_err(map_git_error)?;
        storage.set_review_state(&review_ref, ReviewStatus::Pending)?;
        let _ = head;
        let head = git
            .read_ref(&review_ref)
            .map_err(map_git_error)?
            .unwrap_or_default();
        Ok((review_ref, head))
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

    /// Merge a review head into a feature ref with a true multi-parent
    /// merge. Conflicts advance the feature ref to an unresolved commit
    /// (blocking later main merges); clean merges mark the review
    /// approved.
    pub(crate) fn merge_review_into_feature(
        &self,
        review_ref: &str,
        review_head: &str,
        feature_name: &str,
        actor_str: &str,
    ) -> Result<GitMergeOutcome, CheckpointError> {
        crate::branch::ensure_feature_branch_name(feature_name)?;
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let feature_ref = feat_ref_for_name(feature_name);
        let feature_head = git.read_ref(&feature_ref).map_err(map_git_error)?;
        if let Some(head) = &feature_head {
            let commit = git.read_commit(head).map_err(map_git_error)?;
            if commit.trailer(TRAILER_STATE).as_deref() == Some(STATE_CONFLICT_UNRESOLVED) {
                return Err(CheckpointError::MergeConflict {
                    actor: actor_str.to_string(),
                    files: commit.trailers(TRAILER_CONFLICT_FILE),
                });
            }
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
        let base_tree: HashMap<String, Vec<u8>> = match (&feature_head, review_head) {
            (Some(feature), _) => match git
                .merge_base(feature, review_head)
                .map_err(map_git_error)?
            {
                Some(base) => {
                    let commit = git.read_commit(&base).map_err(map_git_error)?;
                    git.tree_to_bytes(&commit.tree).map_err(map_git_error)?
                }
                None => HashMap::new(),
            },
            (None, _) => HashMap::new(),
        };
        let (merged, conflicts) = merge_file_maps(&base_tree, &feature_tree, &review_tree);
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
        let parent_tree = feature_head
            .as_ref()
            .and_then(|h| git.read_commit(h).ok())
            .map(|c| c.tree);
        let merged_tree = git
            .build_tree_from_parent(parent_tree.as_deref(), &changes)
            .map_err(map_git_error)?;
        let mut parents = Vec::new();
        if let Some(head) = feature_head.clone() {
            parents.push(head);
        }
        parents.push(review_head.to_string());
        if conflicts.is_empty() {
            let message = commit_message(
                &format!("merge review into feature '{feature_name}'"),
                Some(actor_str),
                review_commit
                    .trailer(crate::git_store::TRAILER_SESSION)
                    .as_deref(),
                Some("review"),
                &[],
            );
            let id = git
                .write_commit(
                    &merged_tree,
                    &parents,
                    actor_str,
                    crate::git_store::SYSTEM_COMMITTER,
                    now_millis(),
                    &message,
                )
                .map_err(map_git_error)?;
            git.write_ref(&feature_ref, &id).map_err(map_git_error)?;
            storage.set_review_state(review_ref, ReviewStatus::Approved)?;
            self.index_commit(storage, &id, actor_str, "", "review", &all_paths)?;
            return Ok(GitMergeOutcome {
                commit_id: id,
                parents,
                conflict_files: Vec::new(),
                details: Vec::new(),
            });
        }
        let mut trailers = vec![(
            TRAILER_STATE.to_string(),
            STATE_CONFLICT_UNRESOLVED.to_string(),
        )];
        for file in &conflicts {
            trailers.push((TRAILER_CONFLICT_FILE.to_string(), file.clone()));
        }
        let message = commit_message(
            &format!("merge review into feature '{feature_name}' (conflicted)"),
            Some(actor_str),
            review_commit
                .trailer(crate::git_store::TRAILER_SESSION)
                .as_deref(),
            Some("review"),
            &trailers,
        );
        let id = git
            .write_commit(
                &merged_tree,
                &parents,
                actor_str,
                crate::git_store::SYSTEM_COMMITTER,
                now_millis(),
                &message,
            )
            .map_err(map_git_error)?;
        git.write_ref(&feature_ref, &id).map_err(map_git_error)?;
        storage.set_review_state(review_ref, ReviewStatus::Pending)?;
        self.index_commit(storage, &id, actor_str, "", "review", &all_paths)?;
        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::merge_conflicted(
                id.clone(),
                conflicts.clone(),
                Some(actor_str.to_string()),
            ));
        }
        if let Some(root) = self.workspace_root.clone() {
            let _ = self.materialize_files_into(&merged, &conflicts, &root);
        }
        let details = conflicts
            .iter()
            .map(|file| GitConflictDetail {
                file: file.clone(),
                ours_lines: feature_tree
                    .get(file)
                    .map(|b| split_lines(b))
                    .unwrap_or_default(),
                theirs_lines: review_tree
                    .get(file)
                    .map(|b| split_lines(b))
                    .unwrap_or_default(),
            })
            .collect();
        Ok(GitMergeOutcome {
            commit_id: id,
            parents,
            conflict_files: conflicts,
            details,
        })
    }

    /// Merge one feature ref into main (single sequential step; callers
    /// iterate for multiple features so each keeps its own merge commit).
    /// Refuses while either side is unresolved; conflicts stay on the
    /// feature ref and never create a main commit.
    pub(crate) fn merge_feature_into_main(
        &self,
        feature_name: &str,
        actor_str: &str,
    ) -> Result<GitMergeOutcome, CheckpointError> {
        crate::branch::ensure_feature_branch_name(feature_name)?;
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let feature_ref = feat_ref_for_name(feature_name);
        let feature_head = git
            .read_ref(&feature_ref)
            .map_err(map_git_error)?
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("feature '{feature_name}'"),
            })?;
        let feature_commit = git.read_commit(&feature_head).map_err(map_git_error)?;
        if feature_commit.trailer(TRAILER_STATE).as_deref() == Some(STATE_CONFLICT_UNRESOLVED) {
            return Err(CheckpointError::MergeConflict {
                actor: actor_str.to_string(),
                files: feature_commit.trailers(TRAILER_CONFLICT_FILE),
            });
        }
        let main_head = git.read_ref(REF_MAIN).map_err(map_git_error)?;
        if let Some(head) = &main_head {
            let commit = git.read_commit(head).map_err(map_git_error)?;
            if commit.trailer(TRAILER_STATE).as_deref() == Some(STATE_CONFLICT_UNRESOLVED) {
                return Err(CheckpointError::MergeConflict {
                    actor: actor_str.to_string(),
                    files: commit.trailers(TRAILER_CONFLICT_FILE),
                });
            }
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
        let (merged, conflicts) = merge_file_maps(&base_tree, &main_tree, &feature_tree);
        if !conflicts.is_empty() {
            // Blocked: record the conflicted merge on the feature ref (so
            // it is enumerable and resolvable) without touching main.
            let mut changes: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
            for (path, content) in &merged {
                changes.insert(
                    path.clone(),
                    content
                        .clone()
                        .map(|bytes| (crate::git_store::MODE_FILE.to_string(), bytes)),
                );
            }
            let parent_tree = Some(feature_commit.tree.clone());
            let merged_tree = git
                .build_tree_from_parent(parent_tree.as_deref(), &changes)
                .map_err(map_git_error)?;
            let parents = vec![feature_head.clone()];
            let mut trailers = vec![(
                TRAILER_STATE.to_string(),
                STATE_CONFLICT_UNRESOLVED.to_string(),
            )];
            for file in &conflicts {
                trailers.push((TRAILER_CONFLICT_FILE.to_string(), file.clone()));
            }
            let message = commit_message(
                &format!("merge feature '{feature_name}' into main (conflicted)"),
                Some(actor_str),
                None,
                Some("merge"),
                &trailers,
            );
            let id = git
                .write_commit(
                    &merged_tree,
                    &parents,
                    actor_str,
                    crate::git_store::SYSTEM_COMMITTER,
                    now_millis(),
                    &message,
                )
                .map_err(map_git_error)?;
            git.write_ref(&feature_ref, &id).map_err(map_git_error)?;
            let mut all_paths: Vec<String> = merged.keys().cloned().collect();
            all_paths.sort();
            self.index_commit(storage, &id, actor_str, "", "merge", &all_paths)?;
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::merge_conflicted(
                    id.clone(),
                    conflicts.clone(),
                    Some(actor_str.to_string()),
                ));
            }
            if let Some(root) = self.workspace_root.clone() {
                let _ = self.materialize_files_into(&merged, &conflicts, &root);
            }
            return Err(CheckpointError::MergeConflict {
                actor: actor_str.to_string(),
                files: conflicts,
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
        let parent_tree = main_head
            .as_ref()
            .and_then(|h| git.read_commit(h).ok())
            .map(|c| c.tree);
        let merged_tree = git
            .build_tree_from_parent(parent_tree.as_deref(), &changes)
            .map_err(map_git_error)?;
        let mut parents = Vec::new();
        if let Some(head) = main_head.clone() {
            parents.push(head);
        }
        parents.push(feature_head.clone());
        let message = commit_message(
            &format!("merge feature '{feature_name}' into main"),
            Some(actor_str),
            None,
            Some("merge"),
            &[],
        );
        let id = git
            .write_commit(
                &merged_tree,
                &parents,
                actor_str,
                crate::git_store::SYSTEM_COMMITTER,
                now_millis(),
                &message,
            )
            .map_err(map_git_error)?;
        git.write_ref(REF_MAIN, &id).map_err(map_git_error)?;
        self.index_commit(storage, &id, actor_str, "", "merge", &all_paths)?;
        Ok(GitMergeOutcome {
            commit_id: id,
            parents,
            conflict_files: Vec::new(),
            details: Vec::new(),
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
        let storage = self.storage_ref()?;
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
        let parents = vec![main_head];
        let mut trailers = Vec::new();
        if !conflicts.is_empty() {
            trailers.push((
                TRAILER_STATE.to_string(),
                STATE_CONFLICT_UNRESOLVED.to_string(),
            ));
            for file in &conflicts {
                trailers.push((TRAILER_CONFLICT_FILE.to_string(), file.clone()));
            }
        }
        let message = commit_message(
            &format!("revert feature merge {feature_head}"),
            Some(actor_str),
            None,
            Some("rollback"),
            &trailers,
        );
        let id = git
            .write_commit(
                &merged_tree,
                &parents,
                actor_str,
                crate::git_store::SYSTEM_COMMITTER,
                now_millis(),
                &message,
            )
            .map_err(map_git_error)?;
        git.write_ref(REF_MAIN, &id).map_err(map_git_error)?;
        self.index_commit(storage, &id, actor_str, "", "rollback", &all_paths)?;
        let details = conflicts
            .iter()
            .map(|file| GitConflictDetail {
                file: file.clone(),
                ours_lines: main_tree
                    .get(file)
                    .map(|b| split_lines(b))
                    .unwrap_or_default(),
                theirs_lines: pre_merge_tree
                    .get(file)
                    .map(|b| split_lines(b))
                    .unwrap_or_default(),
            })
            .collect();
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

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
