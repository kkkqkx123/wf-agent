use layertwine::checkpoint::types::{Checkpoint, CheckpointMetadata};
use layertwine::core::types::CheckpointId;
use layertwine::layered::agent;
use layertwine::storage::repository::{CheckpointPersist, PartitionStore};
use layertwine::storage::sqlite::SqliteStorage;

use crate::branch::execution_branch_name;
use crate::file::util::{
    map_layertwine_error, map_layertwine_error_with, partition_latest_snapshot_ids,
    projection as projection_fn,
};
use crate::file::{FileCheckpoint, FileCheckpointManager, FileContentEntry};
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    // ── checkpoint creation ─────────────────────────────────────────

    /// Create a file checkpoint for an entity: apply each entry as an agent
    /// edit on the actor partition, snapshot the partition state into a
    /// layertwine `Checkpoint` (`metadata.author = ActorId`, parent = the
    /// actor's previous checkpoint, forming a linear commit chain) and
    /// return the projection.
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
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(&actor)?;
        // One operation creates one edit group: every entry of this
        // checkpoint is grouped so the whole multi-file operation can be
        // listed and rolled back atomically.
        let session_id = self.begin_edit_group(Some("file checkpoint".to_string()))?;
        for entry in entries {
            let path = crate::file::util::validate_workspace_relative_path(&entry.path)?;
            if entry.deleted {
                self.apply_agent_delete_in_session(&actor, &path, &session_id)?;
            } else {
                self.apply_agent_edit_in_session(&actor, &path, &entry.content, &session_id)?;
            }
        }
        let partition = storage
            .get_partition(&agent::agent_partition_id(&agent_id))
            .map_err(|e| map_layertwine_error_with("create_checkpoint.get_partition", e))?;
        let baseline_snapshots = partition_latest_snapshot_ids(storage, &partition)?;
        let parents: Vec<CheckpointId> = self
            .latest_checkpoint_id(storage, &actor)?
            .into_iter()
            .filter_map(|id| CheckpointId::from_hex(&id))
            .collect();
        let chain_length = parents.len() as u64 + 1;
        let is_full = parents.is_empty();
        let checkpoint = Checkpoint::new_at(
            baseline_snapshots,
            parents,
            CheckpointMetadata::new(actor.as_str(), "file checkpoint"),
            self.creation_timestamp()?,
        );
        self.store
            .branch_adapter
            .store_file_history_checkpoint(&checkpoint)?;
        // Single truth: DB row plus branch head are authoritative, the
        // in-memory map is only a lookup cache. The branch head advances
        // only for explicitly prepared execution branches (created by
        // `ensure_child_branch`): checkpoint creation never implicitly
        // registers a branch, so root executions stay branchless.
        self.store
            .latest_checkpoints
            .insert(actor.as_str().to_string(), checkpoint.id.to_hex());
        let branch_name = execution_branch_name("execution", entity_id);
        if self.store.branch_adapter.branch_exists_now(&branch_name)? {
            // Monotonic head advancement only: a commit that does not
            // descend from the current head (skewed or abandoned line) must
            // not steal the branch pointer.
            self.store
                .branch_adapter
                .advance_branch_head_if_descendant(&branch_name, &checkpoint.id.to_hex())?;
        }
        Ok((self.project(storage, &checkpoint)?, chain_length, is_full))
    }

    /// Create a file checkpoint for an entity from the actor partition's
    /// current state (the deferred snapshot path used by async
    /// persistence). Returns `None` when the entity has no file history yet.
    pub fn create_latest_file_checkpoint(
        &self,
        entity_id: &str,
    ) -> Result<Option<FileCheckpoint>, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        let partition = match storage.get_partition(&agent::agent_partition_id(&agent_id)) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        if partition.history.len() <= 1 {
            return Ok(None);
        }
        let baseline_snapshots = partition_latest_snapshot_ids(storage, &partition)?;
        let parents = self
            .latest_checkpoint_id(storage, &actor)?
            .into_iter()
            .filter_map(|id| CheckpointId::from_hex(&id))
            .collect();
        let checkpoint = Checkpoint::new_at(
            baseline_snapshots,
            parents,
            CheckpointMetadata::new(actor.as_str(), "file checkpoint"),
            self.creation_timestamp()?,
        );
        self.store
            .branch_adapter
            .store_file_history_checkpoint(&checkpoint)?;
        // Single truth: DB row plus branch head are authoritative, the
        // in-memory map is only a lookup cache. As in `create_checkpoint`,
        // the head advances only for prepared execution branches; root
        // executions never gain a branch implicitly.
        self.store
            .latest_checkpoints
            .insert(actor.as_str().to_string(), checkpoint.id.to_hex());
        let branch_name = execution_branch_name("execution", entity_id);
        if self.store.branch_adapter.branch_exists_now(&branch_name)? {
            // Monotonic head advancement only: a commit that does not
            // descend from the current head (skewed or abandoned line) must
            // not steal the branch pointer.
            self.store
                .branch_adapter
                .advance_branch_head_if_descendant(&branch_name, &checkpoint.id.to_hex())?;
        }
        Ok(Some(self.project(storage, &checkpoint)?))
    }

    pub(crate) fn load_checkpoint(
        &self,
        storage: &SqliteStorage,
        checkpoint_id: &str,
    ) -> Result<Checkpoint, CheckpointError> {
        let id =
            CheckpointId::from_hex(checkpoint_id).ok_or_else(|| CheckpointError::Validation {
                reason: format!("invalid checkpoint id '{}'", checkpoint_id),
            })?;
        let exists = storage
            .checkpoint_exists(&id)
            .map_err(map_layertwine_error)?;
        if !exists {
            return Err(CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            });
        }
        storage.get_checkpoint(&id).map_err(map_layertwine_error)
    }

    /// Projection of a layertwine checkpoint with the actor's deletion
    /// markers applied.
    pub(crate) fn project(
        &self,
        storage: &SqliteStorage,
        checkpoint: &Checkpoint,
    ) -> Result<FileCheckpoint, CheckpointError> {
        use crate::file::util::checkpoint_deleted_paths as checkpoint_deleted_paths_fn;
        let deleted = checkpoint_deleted_paths_fn(storage, checkpoint)?;
        projection_fn(storage, checkpoint, &deleted)
    }

    pub(crate) fn latest_checkpoint_id(
        &self,
        _storage: &SqliteStorage,
        actor: &checkpoint_base::actor::id::ActorId,
    ) -> Result<Option<String>, CheckpointError> {
        use layertwine::storage::repository::CheckpointPersist;

        let actor_str = actor.as_str().to_string();
        // Cache first with existence validation, DB head-first selection as
        // cross-process fallback. A cached id removed by another handle or
        // by GC is evicted so callers never build on a dangling parent. When
        // another handle advanced the line, the cache adopts the
        // head-preferred commit so concurrent creators converge instead of
        // forking; a wall-clock-newer orphan never displaces it.
        let cached = self
            .store
            .latest_checkpoints
            .get(&actor_str)
            .map(|entry| entry.clone());
        if let Some(id) = cached {
            match CheckpointId::from_hex(&id) {
                Some(parsed)
                    if self
                        .store
                        .branch_adapter
                        .storage()
                        .checkpoint_exists(&parsed)
                        .unwrap_or(false) =>
                {
                    match self
                        .store
                        .branch_adapter
                        .latest_file_history_id_by_author(&actor_str)
                    {
                        Ok(Some(fresh)) if fresh != id => {
                            self.store
                                .latest_checkpoints
                                .insert(actor_str, fresh.clone());
                            return Ok(Some(fresh));
                        }
                        Ok(_) => return Ok(Some(id)),
                        Err(_) => return Ok(Some(id)),
                    }
                }
                _ => {
                    self.store.latest_checkpoints.remove(&actor_str);
                }
            }
        }
        // Cross-process fallback via the file-history facade.
        let latest = self
            .store
            .branch_adapter
            .latest_file_history_id_by_author(&actor_str)?;
        if let Some(ref id) = latest {
            self.store.latest_checkpoints.insert(actor_str, id.clone());
        }
        Ok(latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, content: &[u8]) -> FileContentEntry {
        FileContentEntry::new(path, content.to_vec())
    }

    /// Baseline snapshots of a stored checkpoint, for crafting test commits
    /// that share content with an existing line.
    fn stored_baselines(
        storage: &layertwine::storage::sqlite::SqliteStorage,
        id: &CheckpointId,
    ) -> Vec<layertwine::core::types::SnapshotId> {
        use layertwine::storage::repository::CheckpointPersist;

        storage.get_checkpoint(id).unwrap().baseline_snapshots
    }

    #[tokio::test]
    async fn checkpoint_updates_branch_head() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        manager
            .create_checkpoint("parent-1", &[entry("a.txt", b"base")])
            .unwrap();
        manager
            .ensure_child_branch("child-1", Some("parent-1"))
            .await
            .unwrap();

        let branch = execution_branch_name("execution", "child-1");
        assert_eq!(
            manager
                .store
                .branch_adapter
                .get_branch_head(&branch)
                .unwrap(),
            None,
            "head must be unset before the first checkpoint"
        );

        let cp1 = manager
            .create_checkpoint("child-1", &[entry("a.txt", b"edit-1")])
            .unwrap();
        assert_eq!(
            manager
                .store
                .branch_adapter
                .get_branch_head(&branch)
                .unwrap()
                .as_deref(),
            Some(cp1.id.as_str())
        );

        // The head follows subsequent checkpoints.
        let cp2 = manager
            .create_checkpoint("child-1", &[entry("a.txt", b"edit-2")])
            .unwrap();
        assert_ne!(cp1.id, cp2.id);
        assert_eq!(
            manager
                .store
                .branch_adapter
                .get_branch_head(&branch)
                .unwrap()
                .as_deref(),
            Some(cp2.id.as_str())
        );
        assert_eq!(
            manager.branch_head("child-1").unwrap().as_deref(),
            Some(cp2.id.as_str())
        );
    }

    #[tokio::test]
    async fn deferred_checkpoint_updates_branch_head() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        manager
            .create_checkpoint("parent-1", &[entry("a.txt", b"base")])
            .unwrap();
        manager
            .ensure_child_branch("child-1", Some("parent-1"))
            .await
            .unwrap();
        // Give the child partition history so the deferred snapshot path has
        // something to project (two entries: seed + edit).
        manager
            .create_checkpoint("child-1", &[entry("a.txt", b"edit-1")])
            .unwrap();

        let branch = execution_branch_name("execution", "child-1");
        let head_before = manager
            .store
            .branch_adapter
            .get_branch_head(&branch)
            .unwrap();

        let deferred = manager
            .create_latest_file_checkpoint("child-1")
            .unwrap()
            .expect("partition history exists");

        let head_after = manager
            .store
            .branch_adapter
            .get_branch_head(&branch)
            .unwrap();
        assert_eq!(head_after.as_deref(), Some(deferred.id.as_str()));
        assert_ne!(head_after, head_before);
    }

    #[tokio::test]
    async fn concurrent_creators_converge_to_newest() {
        use checkpoint_base::clock::CheckpointClock;
        use std::sync::Arc;

        let storage =
            Arc::new(layertwine::storage::sqlite::SqliteStorage::new_full_in_memory().unwrap());
        // B's clock runs behind A's: the second commit is wall-clock older,
        // yet convergence follows the branch head, not timestamps.
        let manager_a = FileCheckpointManager::with_sqlite(storage.clone())
            .with_clock(CheckpointClock::manual(1_000_000));
        let manager_b = FileCheckpointManager::with_sqlite(storage.clone())
            .with_clock(CheckpointClock::manual(500_000));

        manager_a
            .create_checkpoint("parent-1", &[entry("a.txt", b"base")])
            .unwrap();
        manager_a
            .ensure_child_branch("shared-entity", Some("parent-1"))
            .await
            .unwrap();
        manager_b
            .ensure_child_branch("shared-entity", Some("parent-1"))
            .await
            .unwrap();

        let first = manager_a
            .create_checkpoint("shared-entity", &[entry("a.txt", b"v1")])
            .unwrap();
        let second = manager_b
            .create_checkpoint("shared-entity", &[entry("a.txt", b"v2")])
            .unwrap();
        assert_ne!(first.id, second.id);

        let actor = manager_a.actor_id_for("shared-entity");
        let seen = manager_a
            .latest_checkpoint_id(manager_a.storage().unwrap(), &actor)
            .unwrap();
        assert_eq!(seen.as_deref(), Some(second.id.as_str()));

        let third = manager_a
            .create_checkpoint("shared-entity", &[entry("a.txt", b"v3")])
            .unwrap();
        let stored: Checkpoint = manager_a
            .storage()
            .unwrap()
            .get_checkpoint(&CheckpointId::from_hex(&third.id).unwrap())
            .unwrap();
        assert!(
            stored.parents.iter().any(|p| p.to_hex() == second.id),
            "third checkpoint must chain onto the newest commit"
        );
    }

    #[test]
    fn head_beats_wall_clock_newer_orphan() {
        use checkpoint_base::clock::CheckpointClock;
        use layertwine::checkpoint::types::CheckpointMetadata;
        use layertwine::storage::repository::CheckpointPersist;

        let clock = CheckpointClock::manual(1_000_000);
        let handle = clock.manual_handle().expect("manual clock");
        let manager = FileCheckpointManager::new_in_memory()
            .unwrap()
            .with_clock(clock);
        let storage = manager.storage().unwrap();
        let actor = manager.actor_id_for("child-1");
        let head_cp = manager
            .create_checkpoint("child-1", &[entry("a.txt", b"v1")])
            .unwrap();
        let head_id = CheckpointId::from_hex(&head_cp.id).unwrap();

        // An orphan on a sideways line with a newer timestamp must not
        // displace the branch tip (no branch exists for root executions, so
        // manufacture the head row directly).
        manager
            .store
            .branch_adapter
            .set_branch_head("execution/child-1", &head_cp.id)
            .unwrap();
        handle.advance(100_000);
        let orphan = layertwine::checkpoint::Checkpoint::new_at(
            stored_baselines(storage, &head_id),
            vec![head_id],
            CheckpointMetadata::new(actor.as_str(), "orphan line"),
            handle.current_ms(),
        );
        storage.store_checkpoint(&orphan).unwrap();

        let seen = manager
            .latest_checkpoint_id(storage, &actor)
            .unwrap();
        assert_eq!(seen.as_deref(), Some(head_cp.id.as_str()));

        // The orphan descends from the head, so the head legitimately
        // advances to it on the next write that chains onto it.
        let advanced = manager
            .store
            .branch_adapter
            .advance_branch_head_if_descendant("execution/child-1", &orphan.id.to_hex())
            .unwrap();
        assert!(advanced);
    }

    #[test]
    fn head_advance_rejects_foreign_line() {
        use checkpoint_base::clock::CheckpointClock;
        use layertwine::checkpoint::types::CheckpointMetadata;
        use layertwine::storage::repository::CheckpointPersist;

        let manager = FileCheckpointManager::new_in_memory()
            .unwrap()
            .with_clock(CheckpointClock::manual(1_000_000));
        let storage = manager.storage().unwrap();
        let actor = manager.actor_id_for("child-1");
        let head_cp = manager
            .create_checkpoint("child-1", &[entry("a.txt", b"v1")])
            .unwrap();
        manager
            .store
            .branch_adapter
            .set_branch_head("execution/child-1", &head_cp.id)
            .unwrap();

        // A commit from a disjoint history shares no ancestry with the head.
        let foreign = layertwine::checkpoint::Checkpoint::new_at(
            stored_baselines(storage, &CheckpointId::from_hex(&head_cp.id).unwrap()),
            vec![],
            CheckpointMetadata::new(actor.as_str(), "foreign line"),
            2_000_000,
        );
        storage.store_checkpoint(&foreign).unwrap();

        let advanced = manager
            .store
            .branch_adapter
            .advance_branch_head_if_descendant("execution/child-1", &foreign.id.to_hex())
            .unwrap();
        assert!(!advanced, "foreign line must not steal the head");
        assert_eq!(
            manager
                .store
                .branch_adapter
                .get_branch_head("execution/child-1")
                .unwrap()
                .as_deref(),
            Some(head_cp.id.as_str())
        );
        let seen = manager.latest_checkpoint_id(storage, &actor).unwrap();
        assert_eq!(seen.as_deref(), Some(head_cp.id.as_str()));
    }

    #[test]
    fn stale_cache_entry_is_evicted_when_checkpoint_missing() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.actor_id_for("entity-1");
        manager
            .store
            .latest_checkpoints
            .insert(actor.as_str().to_string(), "00".repeat(32));
        let seen = manager
            .latest_checkpoint_id(manager.storage().unwrap(), &actor)
            .unwrap();
        assert_eq!(seen, None);
        assert!(manager
            .store
            .latest_checkpoints
            .get(actor.as_str())
            .is_none());
    }
}
