//! Edit groups and local undo on the Git model.
//!
//! A session is an in-memory staging batch: `begin_edit_group` allocates a
//! token, `apply_*_in_session` validate and stage bytes without committing,
//! and `commit_edit_group` produces the single atomic commit carrying the
//! session trailer. No session association rows are written.
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

/// Opaque edit-group token. Sessions are in-memory staging batches, never
/// persistence rows; the committed group is identified by its session
/// trailer instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EditSessionId(pub String);

impl EditSessionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for EditSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for EditSessionId {
    type Err = CheckpointError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Err(CheckpointError::Validation {
                reason: "edit session id must not be empty".to_string(),
            });
        }
        Ok(Self(s.to_string()))
    }
}

/// A staged (not yet committed) edit group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditSession {
    /// Unique session identifier.
    pub id: EditSessionId,
    /// Human-readable label (e.g. "format file", "refactor module").
    #[serde(default)]
    pub label: Option<String>,
    /// Timestamp when the session was created (millis since epoch).
    pub created_at: i64,
}

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_session_id(timestamp: i64) -> EditSessionId {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    EditSessionId(format!("sess-{timestamp}-{n}"))
}

impl FileCheckpointManager {
    // ── edit groups (operation batches) ─────────────────────────────

    /// Begin a new edit group for grouping a multi-file operation.
    /// Returns the group id. The group lives in memory only; staged files
    /// commit atomically via [`Self::commit_edit_group`].
    pub fn begin_edit_group(
        &self,
        label: Option<String>,
    ) -> Result<EditSessionId, CheckpointError> {
        let timestamp = self.creation_timestamp().unwrap_or_else(|_| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        });
        let id = next_session_id(timestamp);
        self.pending_batches.insert(
            id.to_string(),
            PendingEditBatch {
                label,
                actor: String::new(),
                created_at: timestamp,
                files: HashMap::new(),
            },
        );
        Ok(id)
    }

    /// List staged (not yet committed) edit sessions, newest first.
    pub fn list_sessions(&self) -> Result<Vec<EditSession>, CheckpointError> {
        let mut sessions: Vec<EditSession> = self
            .pending_batches
            .iter()
            .map(|entry| {
                let id: EditSessionId = EditSessionId(entry.key().clone());
                EditSession {
                    id,
                    label: entry.value().label.clone(),
                    created_at: entry.value().created_at,
                }
            })
            .collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.created_at));
        Ok(sessions)
    }

    fn stage_in_session(
        &self,
        actor: &ActorId,
        path: &str,
        content: Option<Vec<u8>>,
        session_id: &EditSessionId,
    ) -> Result<(), CheckpointError> {
        let validated = crate::file::util::validate_workspace_relative_path(path)?;
        let mut batch = self
            .pending_batches
            .get_mut(&session_id.to_string())
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("edit session {session_id}"),
            })?;
        if batch.actor.is_empty() {
            batch.actor = actor.as_str().to_string();
        } else if batch.actor != actor.as_str() {
            return Err(CheckpointError::Validation {
                reason: format!(
                    "edit session {session_id} belongs to '{}', not '{}'",
                    batch.actor,
                    actor.as_str()
                ),
            });
        }
        batch.files.insert(validated, content);
        Ok(())
    }

    /// Stage one file edit for an actor inside an existing session.
    /// Nothing is committed yet; returns the session id string.
    pub fn apply_agent_edit_in_session(
        &self,
        actor: &ActorId,
        path: &str,
        content: &[u8],
        session_id: &EditSessionId,
    ) -> Result<String, CheckpointError> {
        self.stage_in_session(actor, path, Some(content.to_vec()), session_id)?;
        Ok(session_id.to_string())
    }

    /// Stage one file deletion for an actor inside an existing session.
    /// Returns the session id string.
    pub fn apply_agent_delete_in_session(
        &self,
        actor: &ActorId,
        path: &str,
        session_id: &EditSessionId,
    ) -> Result<String, CheckpointError> {
        self.stage_in_session(actor, path, None, session_id)?;
        Ok(session_id.to_string())
    }

    /// Commit a staged edit group as one atomic commit on the actor's edit
    /// ref, carrying the session trailer. Returns the commit id (hex).
    /// An empty group commits nothing and returns the current head, if any.
    pub fn commit_edit_group(
        &self,
        entity_id: &str,
        session_id: &EditSessionId,
        tool: Option<&str>,
    ) -> Result<String, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let (_, batch) = self
            .pending_batches
            .remove(&session_id.to_string())
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("edit session {session_id}"),
            })?;
        if !batch.actor.is_empty() && batch.actor != actor.as_str() {
            return Err(CheckpointError::Validation {
                reason: format!(
                    "edit session {session_id} belongs to '{}', not '{}'",
                    batch.actor,
                    actor.as_str()
                ),
            });
        }
        if batch.files.is_empty() {
            let git = self.git_ref()?;
            let head = git
                .read_ref(&edit_ref_for_actor(actor.as_str()))
                .map_err(map_git_error)?;
            return head.ok_or_else(|| CheckpointError::NotFound {
                id: format!("no commits for actor '{}'", actor.as_str()),
            });
        }
        let session = session_id.to_string();
        let outcome =
            self.commit_tool_files(&actor, &batch.files, Some(&session), tool, "grouped edit")?;
        Ok(outcome.id)
    }

    /// Roll back an entire session on an actor's edit ref: drop the staged
    /// batch when it is still pending; otherwise move the actor's own ref
    /// back past the contiguous run of head commits carrying the session
    /// trailer. Returns the ref head after rollback (hex).
    pub fn rollback_session(
        &self,
        entity_id: &str,
        session_id: &EditSessionId,
    ) -> Result<String, CheckpointError> {
        // A still-pending batch simply evaporates.
        if self
            .pending_batches
            .remove(&session_id.to_string())
            .is_some()
        {
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
        let target = session_id.to_string();
        let mut cursor = head.clone();
        let mut stepped = false;
        loop {
            let commit = git.read_commit(&cursor).map_err(map_git_error)?;
            if commit.trailer(crate::git_store::TRAILER_SESSION).as_deref() != Some(target.as_str())
            {
                break;
            }
            let Some(parent) = commit.parents.first().cloned() else {
                break;
            };
            cursor = parent;
            stepped = true;
        }
        if !stepped {
            return Err(CheckpointError::Validation {
                reason: format!(
                    "session {session_id} is not at the head of '{}'",
                    actor.as_str()
                ),
            });
        }
        if cursor == head {
            return Ok(head);
        }
        git.write_ref(&refname, &cursor).map_err(map_git_error)?;
        self.redo_stacks
            .entry(actor.as_str().to_string())
            .or_default()
            .push(head);
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
        git.write_ref(&refname, &parent).map_err(map_git_error)?;
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
        git.write_ref(&refname, &next).map_err(map_git_error)?;
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
