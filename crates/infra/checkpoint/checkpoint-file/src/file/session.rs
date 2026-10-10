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
use crate::git_store::edit_ref_for_actor;
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
        let head = git
            .read_ref(&refname)
            .map_err(map_git_error)?
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("no commits for actor '{}'", actor.as_str()),
            })?;
        let target = group_id.to_string();
        let mut cursor = head.clone();
        let mut stepped = false;
        let mut visited: Vec<String> = Vec::new();
        loop {
            let commit = git.read_commit(&cursor).map_err(map_git_error)?;
            if commit.trailer(crate::git_store::TRAILER_SESSION).as_deref() != Some(target.as_str())
            {
                break;
            }
            visited.push(cursor.clone());
            let Some(parent) = commit.parents.first().cloned() else {
                let current = git.read_ref(&refname).map_err(map_git_error)?;
                if current.as_deref() != Some(head.as_str()) {
                    return Err(CheckpointError::Branch(format!(
                        "concurrent ref update conflict on '{refname}'"
                    )));
                }
                git.delete_ref(&refname).map_err(map_git_error)?;
                let mut stack = self
                    .redo_stacks
                    .entry(actor.as_str().to_string())
                    .or_default();
                for id in visited {
                    stack.push(id);
                }
                self.store.latest_checkpoints.remove(actor.as_str());
                return Ok(head);
            };
            cursor = parent;
            stepped = true;
        }
        if !stepped {
            return Err(CheckpointError::Validation {
                reason: format!(
                    "edit group {group_id} is not at the head of '{}'",
                    actor.as_str()
                ),
            });
        }
        if cursor == head {
            return Ok(head);
        }
        git.compare_and_swap(&refname, Some(head.as_str()), &cursor)
            .map_err(map_git_error)?;
        let mut stack = self
            .redo_stacks
            .entry(actor.as_str().to_string())
            .or_default();
        for id in visited {
            stack.push(id);
        }
        self.store
            .latest_checkpoints
            .insert(actor.as_str().to_string(), cursor.clone());
        Ok(cursor)
    }

    // ── undo / redo (edit-ref cursor) ──────────────────────────────────

    /// Undo the last commit on an actor's edit ref (pushes onto the redo
    /// stack). Returns the new head (hex).
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
        git.compare_and_swap(&refname, Some(head.as_str()), &parent)
            .map_err(map_git_error)?;
        self.redo_stacks
            .entry(actor.as_str().to_string())
            .or_default()
            .push(head);
        self.store
            .latest_checkpoints
            .insert(actor.as_str().to_string(), parent.clone());
        Ok(parent)
    }

    /// Redo the most recently undone commit on an actor's edit ref.
    /// Returns the restored head (hex).
    pub fn redo_edit(&self, entity_id: &str) -> Result<String, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let next = self
            .redo_stacks
            .get_mut(actor.as_str())
            .and_then(|mut stack| stack.pop())
            .ok_or_else(|| CheckpointError::Validation {
                reason: format!("no redo available for actor '{}'", actor.as_str()),
            })?;
        let git = self.git_ref()?;
        let refname = edit_ref_for_actor(actor.as_str());
        let current = git.read_ref(&refname).map_err(map_git_error)?;
        git.compare_and_swap(&refname, current.as_deref(), &next)
            .map_err(map_git_error)?;
        self.store
            .latest_checkpoints
            .insert(actor.as_str().to_string(), next.clone());
        Ok(next)
    }

    /// Whether a redo is available on an actor's edit ref.
    pub fn can_redo(&self, entity_id: &str) -> Result<bool, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        Ok(self
            .redo_stacks
            .get(actor.as_str())
            .is_some_and(|stack| !stack.is_empty()))
    }
}
