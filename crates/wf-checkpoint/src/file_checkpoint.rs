use layertwine::checkpoint::types::{Checkpoint, CheckpointMetadata};
use layertwine::core::types::CheckpointId;
use layertwine::layered::agent;
use layertwine::storage::repository::{CheckpointPersist, PartitionStore};
use layertwine::storage::sqlite::SqliteStorage;

use crate::branch::execution_branch_name;
use crate::error::CheckpointError;
use crate::file::{FileCheckpoint, FileCheckpointManager, FileContentEntry};
use crate::file_util::{
    map_layertwine_error, partition_latest_snapshot_ids, projection as projection_fn,
};

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
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(&actor)?;
        for entry in entries {
            self.apply_agent_edit(&actor, &entry.path, &entry.content)?;
        }
        let partition = storage
            .get_partition(&agent::agent_partition_id(&agent_id))
            .map_err(map_layertwine_error)?;
        let baseline_snapshots = partition_latest_snapshot_ids(storage, &partition)?;
        let parents = self
            .latest_checkpoint_id(storage, &actor)?
            .into_iter()
            .filter_map(|id| CheckpointId::from_hex(&id))
            .collect();
        let checkpoint = Checkpoint::new(
            baseline_snapshots,
            parents,
            CheckpointMetadata::new(actor.as_str(), "file checkpoint"),
        );
        storage
            .store_checkpoint(&checkpoint)
            .map_err(map_layertwine_error)?;
        self.latest_checkpoints
            .insert(actor.as_str().to_string(), checkpoint.id.to_hex());
        let branch_name = execution_branch_name("execution", entity_id);
        let _ = self.branch_adapter.set_branch_head(&branch_name, &checkpoint.id.to_hex());
        self.project(storage, &checkpoint)
    }

    /// Content-level checkpoint: alias of [`FileCheckpointManager::create_checkpoint`].
    pub fn create_checkpoint_with_content(
        &self,
        entity_id: &str,
        entries: &[FileContentEntry],
    ) -> Result<FileCheckpoint, CheckpointError> {
        self.create_checkpoint(entity_id, entries)
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
        let checkpoint = Checkpoint::new(
            baseline_snapshots,
            parents,
            CheckpointMetadata::new(actor.as_str(), "file checkpoint"),
        );
        storage
            .store_checkpoint(&checkpoint)
            .map_err(map_layertwine_error)?;
        self.latest_checkpoints
            .insert(actor.as_str().to_string(), checkpoint.id.to_hex());
        let branch_name = execution_branch_name("execution", entity_id);
        let _ = self.branch_adapter.set_branch_head(&branch_name, &checkpoint.id.to_hex());
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
        use crate::file_util::checkpoint_deleted_paths as checkpoint_deleted_paths_fn;
        let deleted = checkpoint_deleted_paths_fn(storage, checkpoint)?;
        projection_fn(storage, checkpoint, &deleted)
    }

    pub(crate) fn latest_checkpoint_id(
        &self,
        storage: &SqliteStorage,
        actor: &crate::actor_id::ActorId,
    ) -> Result<Option<String>, CheckpointError> {
        let actor_str = actor.as_str().to_string();
        if let Some(id) = self.latest_checkpoints.get(&actor_str) {
            return Ok(Some(id.clone()));
        }
        // Cross-process fallback: scan stored checkpoints by author.
        let checkpoints = storage.list_checkpoints().map_err(map_layertwine_error)?;
        let latest = checkpoints
            .iter()
            .filter(|c| c.metadata.author == actor_str)
            .max_by_key(|c| c.created_at);
        Ok(latest.map(|c| c.id.to_hex()))
    }
}
