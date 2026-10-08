use crate::file::git_write::map_git_error;
use crate::file::{FileCheckpoint, FileCheckpointManager, FileContentEntry};
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    // ── checkpoint creation ─────────────────────────────────────────

    /// Create a file checkpoint for an entity: stage every entry and
    /// commit once on the actor's edit ref (one operation is one atomic
    /// commit), then return the projection of the new head.
    pub fn create_checkpoint(
        &self,
        entity_id: &str,
        entries: &[FileContentEntry],
    ) -> Result<FileCheckpoint, CheckpointError> {
        let start = std::time::Instant::now();
        let size_bytes: u64 = entries.iter().map(|e| e.content.len() as u64).sum();
        let result = self.create_checkpoint_inner(entity_id, entries);
        let duration_ms = start.elapsed().as_millis() as f64;
        match &result {
            Ok((_, chain_length, is_full)) => {
                if let Some(metrics) = self.checkpoint_metrics() {
                    metrics.record_creation(entity_id, duration_ms, size_bytes, *is_full);
                    metrics.record_chain_length(entity_id, *chain_length);
                }
            }
            Err(_) => {
                if let Some(metrics) = self.checkpoint_metrics() {
                    metrics.record_creation_failure(entity_id);
                }
            }
        }
        result.map(|(checkpoint, _, _)| checkpoint)
    }

    fn create_checkpoint_inner(
        &self,
        entity_id: &str,
        entries: &[FileContentEntry],
    ) -> Result<(FileCheckpoint, u64, bool), CheckpointError> {
        // One operation creates one edit group: every entry of this
        // checkpoint is staged, then committed atomically with the group
        // trailer so the whole multi-file operation rolls back as a unit.
        let session_id = self.begin_edit_group(Some("file checkpoint".to_string()))?;
        let actor = self.actor_id_for(entity_id);
        for entry in entries {
            if entry.deleted {
                self.apply_agent_delete_in_group(&actor, &entry.path, &session_id)?;
            } else {
                self.apply_agent_edit_in_group(&actor, &entry.path, &entry.content, &session_id)?;
            }
        }
        let commit_id = self.commit_edit_group(entity_id, &session_id, Some("checkpoint"))?;
        let git = self.git_ref()?;
        let chain_length = git.log(&commit_id, 0).map_err(map_git_error)?.len() as u64;
        let is_full = chain_length <= 1;
        Ok((self.project_commit(&commit_id)?, chain_length, is_full))
    }

    /// Create a file checkpoint for an entity from the actor's edit-line
    /// head (the deferred projection path used by async persistence).
    /// Returns `None` when the entity has no file history yet.
    pub fn create_latest_file_checkpoint(
        &self,
        entity_id: &str,
    ) -> Result<Option<FileCheckpoint>, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let result = match self.latest_checkpoint_id(&actor) {
            Ok(Some(id)) => self.project_commit(&id).map(Some),
            Ok(None) => Ok(None),
            Err(err) => Err(err),
        };
        if result.is_err() {
            if let Some(metrics) = self.checkpoint_metrics() {
                metrics.record_async_projection_failure(entity_id);
            }
        }
        result
    }

    /// Latest commit id on an actor's edit ref. The in-memory mirror is
    /// validated against the ref before use so callers never build on a
    /// dangling parent; the ref itself is authoritative.
    pub(crate) fn latest_checkpoint_id(
        &self,
        actor: &checkpoint_base::actor::id::ActorId,
    ) -> Result<Option<String>, CheckpointError> {
        let actor_str = actor.as_str().to_string();
        let git = self.git_ref()?;
        let refname = crate::git_store::edit_ref_for_actor(&actor_str);
        let head = git.read_ref(&refname).map_err(map_git_error)?;
        // Clone out of the read guard before any write: holding the guard
        // across `insert`/`remove` deadlocks the shard.
        let _cached: Option<String> = self
            .store
            .latest_checkpoints
            .get(&actor_str)
            .map(|entry| entry.clone());
        match head {
            Some(id) => {
                self.store.latest_checkpoints.insert(actor_str, id.clone());
                Ok(Some(id))
            }
            None => {
                self.store.latest_checkpoints.remove(&actor_str);
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch::execution_branch_name;
    use crate::file::FileContentEntry;

    fn entry(path: &str, content: &[u8]) -> FileContentEntry {
        FileContentEntry::new(path, content.to_vec())
    }

    #[tokio::test]
    async fn checkpoint_advances_edit_ref_not_execution_branch() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        manager
            .create_checkpoint("parent-1", &[entry("a.txt", b"base")])
            .unwrap();
        manager
            .ensure_child_branch("child-1", Some("parent-1"))
            .await
            .unwrap();

        let branch = execution_branch_name("child-1");
        assert_eq!(
            manager
                .store
                .branch_adapter
                .get_branch_head(&branch)
                .unwrap(),
            None,
            "execution branches stay headless until execution state advances them"
        );

        // File commits advance the actor's edit ref instead.
        let cp1 = manager
            .create_checkpoint("child-1", &[entry("a.txt", b"edit-1")])
            .unwrap();
        let actor = manager.actor_id_for("child-1");
        assert_eq!(
            manager.latest_checkpoint_id(&actor).unwrap().as_deref(),
            Some(cp1.id.as_str())
        );

        let cp2 = manager
            .create_checkpoint("child-1", &[entry("a.txt", b"edit-2")])
            .unwrap();
        assert_ne!(cp1.id, cp2.id);
        assert_eq!(
            manager.latest_checkpoint_id(&actor).unwrap().as_deref(),
            Some(cp2.id.as_str())
        );
        // The execution branch is untouched by file commits.
        assert_eq!(
            manager
                .store
                .branch_adapter
                .get_branch_head(&branch)
                .unwrap(),
            None,
            "file commits never move execution branch heads"
        );
    }

    #[tokio::test]
    async fn deferred_checkpoint_projects_edit_head() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        manager
            .create_checkpoint("child-1", &[entry("a.txt", b"edit-1")])
            .unwrap();

        let deferred = manager
            .create_latest_file_checkpoint("child-1")
            .unwrap()
            .expect("edit line exists");
        let map: std::collections::HashMap<&str, &crate::file::FileState> = deferred
            .files
            .iter()
            .map(|f| (f.path.as_str(), f))
            .collect();
        assert_eq!(map["a.txt"].hash, crate::file::util::sha256_hex(b"edit-1"));

        assert!(manager
            .create_latest_file_checkpoint("never-touched")
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn concurrent_creators_converge_to_newest() {
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let git = Arc::new(crate::git_store::GitStore::init_for_workspace(dir.path()).unwrap());
        let mut manager_a = FileCheckpointManager::new_in_memory().unwrap();
        manager_a.store.git = Some(git.clone());
        let mut manager_b = FileCheckpointManager::new_in_memory().unwrap();
        manager_b.store.git = Some(git.clone());

        // Both handles share one bare repository: the second commit wins
        // the ref, and the third chains onto it (no fork).
        let first = manager_a
            .create_checkpoint("shared-entity", &[entry("a.txt", b"v1")])
            .unwrap();
        let second = manager_b
            .create_checkpoint("shared-entity", &[entry("a.txt", b"v2")])
            .unwrap();
        assert_ne!(first.id, second.id);

        let actor = manager_a.actor_id_for("shared-entity");
        let seen = manager_a.latest_checkpoint_id(&actor).unwrap();
        assert_eq!(seen.as_deref(), Some(second.id.as_str()));

        let third = manager_a
            .create_checkpoint("shared-entity", &[entry("a.txt", b"v3")])
            .unwrap();
        let stored = git.read_commit(&third.id).unwrap();
        assert!(
            stored.parents.iter().any(|p| p == &second.id),
            "third commit must chain onto the newest commit"
        );
    }

    #[test]
    fn execution_branch_heads_roundtrip() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let head_cp = manager
            .create_checkpoint("child-1", &[entry("a.txt", b"v1")])
            .unwrap();
        assert_eq!(
            manager
                .store
                .branch_adapter
                .get_branch_head("execution/child-1")
                .unwrap(),
            None
        );
        manager
            .store
            .branch_adapter
            .set_branch_head("execution/child-1", &head_cp.id)
            .unwrap();
        assert_eq!(
            manager
                .store
                .branch_adapter
                .get_branch_head("execution/child-1")
                .unwrap()
                .as_deref(),
            Some(head_cp.id.as_str())
        );
        // File lines are unaffected by execution branch pointers.
        let actor = manager.actor_id_for("child-1");
        let seen = manager.latest_checkpoint_id(&actor).unwrap();
        assert_eq!(seen.as_deref(), Some(head_cp.id.as_str()));
    }

    #[test]
    fn stale_cache_entry_is_evicted_when_ref_missing() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.actor_id_for("entity-1");
        manager
            .store
            .latest_checkpoints
            .insert(actor.as_str().to_string(), "00".repeat(32));
        let seen = manager.latest_checkpoint_id(&actor).unwrap();
        assert_eq!(seen, None);
        assert!(manager
            .store
            .latest_checkpoints
            .get(actor.as_str())
            .is_none());
    }
}
