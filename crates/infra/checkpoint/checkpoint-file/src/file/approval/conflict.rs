//! Approval orchestration on the commit DAG.
//!
//! Write domain: submit, approve, reject and merge review refs into features.
//! Read models live in the top-level approval module.

use std::collections::HashMap;

use crate::event::{ApprovalDecision, CheckpointEventBus};
use crate::file::git_write::map_git_error;
use crate::file::util::validate_workspace_relative_path;
use crate::file::FileCheckpointManager;
use crate::git_store::{feat_ref_for_name, REF_REVIEW_PREFIX};
use crate::storage::ReviewStatus;
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
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
        let resolved_set: std::collections::HashSet<&str> =
            paths.iter().map(String::as_str).collect();
        let mut still_marked = Vec::new();
        for (path, content) in &resolved_contents {
            if !crate::file::git_merge::parse_conflict_regions(content).is_empty() {
                still_marked.push(path.clone());
            }
        }
        still_marked.sort();
        let mut remaining_files: Vec<String> = conflicted
            .iter()
            .filter(|file| !resolved_set.contains(file.as_str()))
            .cloned()
            .collect();
        remaining_files.extend(still_marked.clone());
        remaining_files.sort();
        remaining_files.dedup();
        let mut state_trailers = Vec::new();
        if remaining_files.is_empty() {
            state_trailers.push((
                crate::git_store::TRAILER_STATE.to_string(),
                crate::git_store::STATE_CONFLICT_RESOLVED.to_string(),
            ));
        } else {
            state_trailers.push((
                crate::git_store::TRAILER_STATE.to_string(),
                crate::git_store::STATE_CONFLICT_UNRESOLVED.to_string(),
            ));
            for file in &remaining_files {
                state_trailers.push((
                    crate::git_store::TRAILER_CONFLICT_FILE.to_string(),
                    file.clone(),
                ));
            }
        }
        let message = crate::git_store::commit_message(
            &format!("resolve conflicts on feature '{feature_name}'"),
            Some(actor.as_str()),
            None,
            Some("review"),
            &state_trailers,
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
        let remaining = remaining_files.len();
        if remaining == 0 && !conflicted.is_empty() {
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
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::approval_resolved(
                    actor.as_str().to_string(),
                    ApprovalDecision::Approved,
                    id.clone(),
                ));
            }
        }
        Ok(remaining)
    }
}
