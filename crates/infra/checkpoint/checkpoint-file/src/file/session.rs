//! Edit groups and local undo on the Git model.
//!
//! An edit group is an in-memory staging batch: `begin_edit_group` allocates a
//! token, `apply_*_in_session` validate and stage bytes without committing,
//! and `commit_edit_group` produces the single atomic commit carrying the
//! group trailer. No group association rows are written.
//!
//! Local undo moves only the actor's own edit ref to its parent commit
//! (redo is an in-memory stack of undone heads). Review, feature and main
//! refs are never touched here.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::file::git_write::{map_git_error, PendingEditBatch};
use crate::file::FileCheckpointManager;
use crate::git_store::{edit_ref_for_actor, GitStoreError};
use checkpoint_base::actor::id::ActorId;
use checkpoint_base::error::CheckpointError;

/// Opaque edit-group token. Edit groups are in-memory staging batches, never
/// persistence rows; the committed group is identified by its group
/// trailer instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EditGroupId(pub String);

impl EditGroupId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for EditGroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for EditGroupId {
    type Err = CheckpointError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Err(CheckpointError::Validation {
                reason: "edit group id must not be empty".to_string(),
            });
        }
        Ok(Self(s.to_string()))
    }
}

/// A staged (not yet committed) edit group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditGroup {
    /// Unique edit group identifier.
    pub id: EditGroupId,
    /// Human-readable label (e.g. "format file", "refactor module").
    #[serde(default)]
    pub label: Option<String>,
    /// Timestamp when the group was created (millis since epoch).
    pub created_at: i64,
}

static GROUP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_edit_group_id(timestamp: i64) -> EditGroupId {
    let n = GROUP_COUNTER.fetch_add(1, Ordering::Relaxed);
    EditGroupId(format!("sess-{timestamp}-{}-{n}", std::process::id()))
}

/// Where an edit group sits relative to its actor's edit-ref head.
/// Returned by [`FileCheckpointManager::edit_group_head_status`] so a
/// non-head group can be reported (current head included) instead of
/// merely rejected, letting the caller retry later or escalate.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EditGroupHeadStatus {
    /// Current head of the actor's edit ref (`None` when the ref is gone).
    pub head: Option<String>,
    /// Whether the group forms the contiguous head run and
    /// `undo_edit_group` would succeed now.
    pub at_head: bool,
    /// Whether the group is still a staged (uncommitted) batch.
    pub pending: bool,
}

impl FileCheckpointManager {
    // ── edit groups (operation batches) ─────────────────────────────

    /// Begin a new edit group for grouping a multi-file operation.
    /// Returns the group id. The group lives in memory only; staged files
    /// commit atomically via [`Self::commit_edit_group`].
    pub fn begin_edit_group(&self, label: Option<String>) -> Result<EditGroupId, CheckpointError> {
        let timestamp = self.creation_timestamp()?;
        for _ in 0..64 {
            let id = next_edit_group_id(timestamp);
            if self.pending_batches.contains_key(&id.to_string()) {
                continue;
            }
            self.pending_batches.insert(
                id.to_string(),
                PendingEditBatch {
                    label,
                    actor: String::new(),
                    created_at: timestamp,
                    files: HashMap::new(),
                },
            );
            return Ok(id);
        }
        Err(CheckpointError::Internal(
            "failed to allocate a unique edit group id".to_string(),
        ))
    }

    /// List staged (not yet committed) edit groups, newest first.
    pub fn list_edit_groups(&self) -> Result<Vec<EditGroup>, CheckpointError> {
        let mut groups: Vec<EditGroup> = self
            .pending_batches
            .iter()
            .map(|entry| {
                let id: EditGroupId = EditGroupId(entry.key().clone());
                EditGroup {
                    id,
                    label: entry.value().label.clone(),
                    created_at: entry.value().created_at,
                }
            })
            .collect();
        groups.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.id.0.cmp(&b.id.0)));
        Ok(groups)
    }

    fn stage_in_group(
        &self,
        actor: &ActorId,
        path: &str,
        content: Option<Vec<u8>>,
        group_id: &EditGroupId,
    ) -> Result<(), CheckpointError> {
        let validated = crate::file::util::validate_workspace_relative_path(path)?;
        let mut batch = self
            .pending_batches
            .get_mut(&group_id.to_string())
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("edit group {group_id}"),
            })?;
        if batch.actor.is_empty() {
            batch.actor = actor.as_str().to_string();
        } else if batch.actor != actor.as_str() {
            return Err(CheckpointError::Validation {
                reason: format!(
                    "edit group {group_id} belongs to '{}', not '{}'",
                    batch.actor,
                    actor.as_str()
                ),
            });
        }
        batch.files.insert(validated, content);
        Ok(())
    }

    /// Stage one file edit for an actor inside an existing edit group.
    /// Nothing is committed yet; returns the group id string.
    pub fn apply_agent_edit_in_group(
        &self,
        actor: &ActorId,
        path: &str,
        content: &[u8],
        group_id: &EditGroupId,
    ) -> Result<String, CheckpointError> {
        self.stage_in_group(actor, path, Some(content.to_vec()), group_id)?;
        Ok(group_id.to_string())
    }

    /// Stage one file deletion for an actor inside an existing edit group.
    /// Returns the group id string.
    pub fn apply_agent_delete_in_group(
        &self,
        actor: &ActorId,
        path: &str,
        group_id: &EditGroupId,
    ) -> Result<String, CheckpointError> {
        self.stage_in_group(actor, path, None, group_id)?;
        Ok(group_id.to_string())
    }

    /// Commit a staged edit group as one atomic commit on the actor's edit
    /// ref, carrying the group trailer. Returns the commit id (hex).
    /// An empty group commits nothing and returns the current head, if any.
    pub fn commit_edit_group(
        &self,
        entity_id: &str,
        group_id: &EditGroupId,
        tool: Option<&str>,
    ) -> Result<String, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let (_, batch) = self
            .pending_batches
            .remove(&group_id.to_string())
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("edit group {group_id}"),
            })?;
        if !batch.actor.is_empty() && batch.actor != actor.as_str() {
            let owner = batch.actor.clone();
            self.pending_batches.insert(group_id.to_string(), batch);
            return Err(CheckpointError::Validation {
                reason: format!(
                    "edit group {group_id} belongs to '{owner}', not '{}'",
                    actor.as_str()
                ),
            });
        }
        if batch.files.is_empty() {
            let git_result = self.git_ref();
            let git = match git_result {
                Ok(git) => git,
                Err(err) => {
                    self.pending_batches.insert(group_id.to_string(), batch);
                    return Err(err);
                }
            };
            match git
                .read_ref(&edit_ref_for_actor(actor.as_str()))
                .map_err(map_git_error)
            {
                Ok(head) => match head {
                    Some(id) => {
                        return Ok(id);
                    }
                    None => {
                        self.pending_batches.insert(group_id.to_string(), batch);
                        return Err(CheckpointError::NotFound {
                            id: format!("no commits for actor '{}'", actor.as_str()),
                        });
                    }
                },
                Err(err) => {
                    self.pending_batches.insert(group_id.to_string(), batch);
                    return Err(err);
                }
            }
        }
        let session = group_id.to_string();
        match self.commit_tool_files(&actor, &batch.files, Some(&session), tool, "grouped edit") {
            Ok(outcome) => Ok(outcome.id),
            Err(err) => {
                self.pending_batches.insert(group_id.to_string(), batch);
                Err(err)
            }
        }
    }

    /// Roll back an entire edit group on an actor's edit ref: drop the staged
    /// batch when it is still pending; otherwise move the actor's own ref
    /// back past the contiguous run of head commits carrying the group
    /// trailer. Returns the ref head after rollback (hex).
    ///
    /// Pointer-move family (`undo_*`): only the actor's own edit ref
    /// moves; review, feature and main refs are never touched. For the
    /// canonical name see [`Self::undo_edit_group`].
    pub fn rollback_edit_group(
        &self,
        entity_id: &str,
        group_id: &EditGroupId,
    ) -> Result<String, CheckpointError> {
        // A still-pending batch simply evaporates.
        if self.pending_batches.remove(&group_id.to_string()).is_some() {
            let actor = self.actor_id_for(entity_id);
            let git = self.git_ref()?;
            let head = git
                .read_ref(&edit_ref_for_actor(actor.as_str()))
                .map_err(map_git_error)?;
            return head.ok_or_else(|| CheckpointError::NotFound {
                id: format!("no commits for actor '{}'", actor.as_str()),
            });
        }
        let actor = self.actor_id_for(entity_id);
        let git = self.git_ref()?;
        let refname = edit_ref_for_actor(actor.as_str());
        let target = group_id.to_string();
        let head = git
            .read_ref(&refname)
            .map_err(map_git_error)?
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("no commits for actor '{}'", actor.as_str()),
            })?;
        let head_commit = git.read_commit(&head).map_err(map_git_error)?;
        if head_commit
            .trailer(crate::git_store::TRAILER_SESSION)
            .as_deref()
            != Some(target.as_str())
        {
            return Err(CheckpointError::Validation {
                reason: format!(
                    "edit group {group_id} is not at the head of '{}' (head is '{head}'); retry after the head moves or escalate",
                    actor.as_str()
                ),
            });
        }
        let deleted = std::cell::Cell::new(false);
        let spec = crate::file::journal::RefUpdateSpec {
            kind: crate::storage::RefOperationKind::EditGroupRollback,
            reason: format!("roll back edit group {group_id}"),
            actor: actor.as_str().to_string(),
            session: target.clone(),
            tool: "undo".to_string(),
            ref_name: refname.clone(),
            old_id: Some(head.clone()),
            new_id: Some(head.clone()),
            related_refs: Vec::new(),
            effects: crate::file::journal::redo_effects_with_head(
                actor.as_str(),
                self.redo_stacks.entries(actor.as_str()),
                Some(head.clone()),
            ),
        };
        self.journaled_ref_update(spec, |update| {
            let current = update
                .old_id
                .as_deref()
                .ok_or_else(|| GitStoreError::InvalidInput("edit ref disappeared".to_string()))?;
            let mut cursor = current.to_string();
            let mut visited = Vec::new();
            loop {
                let commit = git.read_commit(&cursor)?;
                if commit.trailer(crate::git_store::TRAILER_SESSION).as_deref()
                    != Some(target.as_str())
                {
                    break;
                }
                visited.push(cursor.clone());
                let Some(parent) = commit.parents.first().cloned() else {
                    deleted.set(true);
                    update.new_id = None;
                    update.effects = crate::file::journal::redo_effects_with_head(
                        actor.as_str(),
                        self.redo_stacks
                            .snapshot_after_push_many(actor.as_str(), visited),
                        None,
                    );
                    return git.delete_ref(&update.ref_name);
                };
                cursor = parent;
            }
            if cursor == current {
                return Err(GitStoreError::RefConflict(refname.clone()));
            }
            update.new_id = Some(cursor.clone());
            update.effects = crate::file::journal::redo_effects_with_head(
                actor.as_str(),
                self.redo_stacks
                    .snapshot_after_push_many(actor.as_str(), visited),
                Some(cursor.clone()),
            );
            git.compare_and_swap(
                &update.ref_name,
                update.old_id.as_deref(),
                update.new_id.as_deref().unwrap_or_default(),
            )
        })?;
        if deleted.get() {
            return Err(CheckpointError::NotFound {
                id: format!("no commits for actor '{}'", actor.as_str()),
            });
        }
        let result = git.read_ref(&refname).map_err(map_git_error)?;
        result.ok_or_else(|| CheckpointError::NotFound {
            id: format!("no commits for actor '{}'", actor.as_str()),
        })
    }

    /// Canonical pointer-move name for rolling back an edit group.
    /// Identical to [`Self::rollback_edit_group`]; the `rollback_*`
    /// spelling is a legacy alias kept for compatibility.
    pub fn undo_edit_group(
        &self,
        entity_id: &str,
        group_id: &EditGroupId,
    ) -> Result<String, CheckpointError> {
        self.rollback_edit_group(entity_id, group_id)
    }

    /// Describe where an edit group sits relative to the actor's edit-ref
    /// head without moving anything. Returns the current head and whether
    /// the group forms the contiguous head run (i.e. whether
    /// [`Self::undo_edit_group`] would succeed now). A non-head group is
    /// a business outcome, not an error: the caller decides between
    /// retrying later and escalating to a human.
    pub fn edit_group_head_status(
        &self,
        entity_id: &str,
        group_id: &EditGroupId,
    ) -> Result<EditGroupHeadStatus, CheckpointError> {
        if self.pending_batches.contains_key(&group_id.to_string()) {
            let actor = self.actor_id_for(entity_id);
            let git = self.git_ref()?;
            let head = git
                .read_ref(&edit_ref_for_actor(actor.as_str()))
                .map_err(map_git_error)?;
            return Ok(EditGroupHeadStatus {
                head,
                at_head: true,
                pending: true,
            });
        }
        let actor = self.actor_id_for(entity_id);
        let git = self.git_ref()?;
        let refname = edit_ref_for_actor(actor.as_str());
        let head = git.read_ref(&refname).map_err(map_git_error)?;
        let Some(head_id) = head.clone() else {
            return Err(CheckpointError::NotFound {
                id: format!("no commits for actor '{}'", actor.as_str()),
            });
        };
        let commit = git.read_commit(&head_id).map_err(map_git_error)?;
        let at_head = commit.trailer(crate::git_store::TRAILER_SESSION).as_deref()
            == Some(group_id.to_string().as_str());
        Ok(EditGroupHeadStatus {
            head,
            at_head,
            pending: false,
        })
    }

    // ── undo / redo (edit-ref cursor) ──────────────────────────────────

    /// Undo the last commit on an actor's edit ref (pushes onto the redo
    /// stack). Returns the new head (hex). A concurrent commit landing
    /// between the read and the swap retries against the fresh head, so
    /// undo always removes the latest commit; a parentless head is a
    /// business outcome and never retries.
    pub fn undo_edit(&self, entity_id: &str) -> Result<String, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let git = self.git_ref()?;
        let refname = edit_ref_for_actor(actor.as_str());
        let head = git
            .read_ref(&refname)
            .map_err(map_git_error)?
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("no commits for actor '{}'", actor.as_str()),
            })?;
        let commit = git.read_commit(&head).map_err(map_git_error)?;
        let parent =
            commit
                .parents
                .first()
                .cloned()
                .ok_or_else(|| CheckpointError::Validation {
                    reason: format!("actor '{}' has only the initial commit", actor.as_str()),
                })?;
        let redo = self.redo_stacks.snapshot_after_push(actor.as_str(), &head);
        let spec = crate::file::journal::RefUpdateSpec {
            kind: crate::storage::RefOperationKind::UndoEdit,
            reason: "undo last edit".to_string(),
            actor: actor.as_str().to_string(),
            session: String::new(),
            tool: "undo".to_string(),
            ref_name: refname.clone(),
            old_id: Some(head.clone()),
            new_id: Some(parent.clone()),
            related_refs: Vec::new(),
            effects: crate::file::journal::redo_effects_with_head(
                actor.as_str(),
                redo,
                Some(parent.clone()),
            ),
        };
        self.journaled_ref_update(spec, |update| {
            let current = update.old_id.as_deref().ok_or_else(|| {
                crate::git_store::GitStoreError::InvalidInput("edit ref disappeared".to_string())
            })?;
            let commit = git.read_commit(current)?;
            let target = commit.parents.first().cloned().ok_or_else(|| {
                crate::git_store::GitStoreError::InvalidInput(
                    "actor has only the initial commit".to_string(),
                )
            })?;
            update.new_id = Some(target.clone());
            update.effects = crate::file::journal::redo_effects_with_head(
                actor.as_str(),
                self.redo_stacks
                    .snapshot_after_push(actor.as_str(), current),
                Some(target.clone()),
            );
            git.compare_and_swap(
                &update.ref_name,
                update.old_id.as_deref(),
                update.new_id.as_deref().unwrap_or_default(),
            )
        })?;
        Ok(parent)
    }

    /// Redo the most recently undone commit on an actor's edit ref.
    /// Returns the restored head (hex). The stack entry is only popped after
    /// the swap succeeds, so a conflict never loses the redo. Redo restores
    /// onto the exact cursor undo left: the current head must equal the
    /// parent of the redo target, otherwise a `Branch` conflict is returned
    /// and the stack is kept for the caller to decide.
    pub fn redo_edit(&self, entity_id: &str) -> Result<String, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let next =
            self.redo_stacks
                .last(actor.as_str())
                .ok_or_else(|| CheckpointError::Validation {
                    reason: format!("no redo available for actor '{}'", actor.as_str()),
                })?;
        let git = self.git_ref()?;
        let refname = edit_ref_for_actor(actor.as_str());
        let current = git.read_ref(&refname).map_err(map_git_error)?;
        let next_commit = git.read_commit(&next).map_err(map_git_error)?;
        let expected_parent = next_commit.parents.first();
        if expected_parent != current.as_ref() {
            return Err(CheckpointError::Branch(format!(
                "redo conflict on '{refname}': expected cursor '{}', found '{}'; not overwriting",
                expected_parent.cloned().unwrap_or_default(),
                current.as_deref().unwrap_or("<deleted>"),
            )));
        }
        let redo = self.redo_stacks.snapshot_after_pop(actor.as_str());
        let spec = crate::file::journal::RefUpdateSpec {
            kind: crate::storage::RefOperationKind::RedoEdit,
            reason: "redo last undone edit".to_string(),
            actor: actor.as_str().to_string(),
            session: String::new(),
            tool: "redo".to_string(),
            ref_name: refname.clone(),
            old_id: current.clone(),
            new_id: Some(next.clone()),
            related_refs: Vec::new(),
            effects: crate::file::journal::redo_effects_with_head(
                actor.as_str(),
                redo,
                Some(next.clone()),
            ),
        };
        self.journaled_ref_update(spec, |update| {
            if update.old_id.as_ref() != expected_parent {
                return Err(GitStoreError::RefConflict(refname.clone()));
            }
            git.compare_and_swap(
                &update.ref_name,
                update.old_id.as_deref(),
                update.new_id.as_deref().unwrap_or_default(),
            )
        })?;
        Ok(next)
    }

    /// Whether a redo is available on an actor's edit ref.
    pub fn can_redo(&self, entity_id: &str) -> Result<bool, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        Ok(self.redo_stacks.is_available(actor.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::redo_store::RedoStackStore;
    use std::sync::Arc;

    fn manager() -> FileCheckpointManager {
        FileCheckpointManager::new_in_memory().unwrap()
    }

    #[test]
    fn redo_conflicts_when_ref_moved_after_undo() {
        use crate::git_store::SYSTEM_COMMITTER;
        use std::collections::HashMap;
        let manager = manager();
        let actor = manager.actor_id_for("entity-1");
        manager.apply_agent_edit(&actor, "a.txt", b"v1\n").unwrap();
        manager.apply_agent_edit(&actor, "a.txt", b"v2\n").unwrap();
        let undone = manager.undo_edit("entity-1").unwrap();
        let _ = undone;
        // Move the ref without clearing the redo stack (simulates a
        // concurrent writer bypassing the edit API).
        let git = manager.git_ref().unwrap();
        let head = git
            .read_ref(&crate::git_store::edit_ref_for_actor(actor.as_str()))
            .unwrap()
            .unwrap();
        let mut changes = HashMap::new();
        changes.insert(
            "b.txt".to_string(),
            Some((
                crate::git_store::MODE_FILE.to_string(),
                b"concurrent\n".to_vec(),
            )),
        );
        let tree = git.build_tree_from_parent(None, &changes).unwrap();
        let concurrent = git
            .write_commit(
                &tree,
                std::slice::from_ref(&head),
                actor.as_str(),
                SYSTEM_COMMITTER,
                1,
                "concurrent",
            )
            .unwrap();
        git.compare_and_swap(
            &crate::git_store::edit_ref_for_actor(actor.as_str()),
            Some(head.as_str()),
            &concurrent,
        )
        .unwrap();
        let err = manager.redo_edit("entity-1").unwrap_err();
        assert!(matches!(err, CheckpointError::Branch(_)));
        // The conflict keeps the redo entry for the caller to retry.
        assert!(manager.can_redo("entity-1").unwrap());
    }

    #[test]
    fn undo_redo_and_new_commit_journal_final_redo_snapshots() {
        let manager = manager();
        let actor = manager.actor_id_for("entity-1");
        manager.apply_agent_edit(&actor, "a.txt", b"v1\n").unwrap();
        let head = manager.apply_agent_edit(&actor, "a.txt", b"v2\n").unwrap();
        manager.undo_edit("entity-1").unwrap();
        manager.redo_edit("entity-1").unwrap();
        manager.apply_agent_edit(&actor, "a.txt", b"v3\n").unwrap();

        let records = manager.list_ref_operations().unwrap();
        let undo = records
            .iter()
            .find(|record| record.kind == crate::storage::RefOperationKind::UndoEdit)
            .expect("undo record");
        assert_eq!(undo.status, crate::storage::RefOperationStatus::Applied);
        assert_eq!(
            undo.effects
                .redo_stack
                .as_ref()
                .map(|entry| entry.entries.clone()),
            Some(vec![head.clone()])
        );
        let redo = records
            .iter()
            .find(|record| record.kind == crate::storage::RefOperationKind::RedoEdit)
            .expect("redo record");
        assert_eq!(
            redo.effects
                .redo_stack
                .as_ref()
                .map(|entry| entry.entries.clone()),
            Some(Vec::new())
        );
        let commit = records
            .iter()
            .rev()
            .find(|record| record.kind == crate::storage::RefOperationKind::FileCommit)
            .expect("new commit record");
        assert_eq!(
            commit
                .effects
                .redo_stack
                .as_ref()
                .map(|entry| entry.entries.clone()),
            Some(Vec::new())
        );

        let reloaded =
            RedoStackStore::new(Arc::new(manager.storage_ref().unwrap().share())).unwrap();
        assert!(!reloaded.is_available(actor.as_str()));
    }

    #[test]
    fn group_rollback_journals_the_whole_restored_stack() {
        let manager = manager();
        let actor = manager.actor_id_for("entity-1");
        let base = manager
            .apply_agent_edit(&actor, "a.txt", b"base\n")
            .unwrap();
        let group = manager.begin_edit_group(None).unwrap();
        manager
            .apply_agent_edit_in_group(&actor, "a.txt", b"group\n", &group)
            .unwrap();
        let group_head = manager.commit_edit_group("entity-1", &group, None).unwrap();
        manager.undo_edit_group("entity-1", &group).unwrap();

        let record = manager
            .list_ref_operations()
            .unwrap()
            .into_iter()
            .find(|record| record.kind == crate::storage::RefOperationKind::EditGroupRollback)
            .expect("group rollback record");
        assert_eq!(record.status, crate::storage::RefOperationStatus::Applied);
        assert_eq!(
            record.effects.redo_stack.map(|entry| entry.entries),
            Some(vec![group_head.clone()])
        );
        assert_eq!(
            record
                .effects
                .latest_checkpoint
                .and_then(|entry| entry.head),
            Some(base)
        );

        let reloaded =
            RedoStackStore::new(Arc::new(manager.storage_ref().unwrap().share())).unwrap();
        assert_eq!(
            reloaded.last(actor.as_str()).as_deref(),
            Some(group_head.as_str())
        );
    }

    #[test]
    fn redo_restores_undo_cursor_when_unmoved() {
        let manager = manager();
        let actor = manager.actor_id_for("entity-1");
        manager.apply_agent_edit(&actor, "a.txt", b"v1\n").unwrap();
        let head = manager.apply_agent_edit(&actor, "a.txt", b"v2\n").unwrap();
        manager.undo_edit("entity-1").unwrap();
        let restored = manager.redo_edit("entity-1").unwrap();
        assert_eq!(restored, head);
    }
}
