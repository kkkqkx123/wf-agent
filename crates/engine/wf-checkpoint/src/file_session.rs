use layertwine::core::edit_session::EditSession;
use layertwine::core::types::EditSessionId;
use layertwine::storage::repository::{EditSessionStore, PartitionStore, SnapshotStore};

use crate::actor_id::ActorId;
use crate::error::CheckpointError;
use crate::file::FileCheckpointManager;
use crate::file_util::map_layertwine_error;

impl FileCheckpointManager {
    // ── edit sessions (operation batches) ─────────────────────────────

    /// Begin a new edit session for grouping a multi-file operation.
    /// Returns the session id. The session is persisted immediately so it
    /// survives process restarts; deltas/snapshots are appended as the
    /// operation records them.
    pub fn begin_session(&self, label: Option<String>) -> Result<EditSessionId, CheckpointError> {
        let storage = self.storage_ref()?;
        let session = EditSession::new(label);
        let id = session.id;
        storage
            .store_session(&session)
            .map_err(map_layertwine_error)?;
        Ok(id)
    }

    /// List all persisted edit sessions (newest first).
    pub fn list_sessions(&self) -> Result<Vec<EditSession>, CheckpointError> {
        let storage = self.storage_ref()?;
        storage.list_sessions().map_err(map_layertwine_error)
    }

    /// Record one file edit for an actor inside an existing session.
    /// Returns the new snapshot id (hex).
    pub fn apply_agent_edit_in_session(
        &self,
        actor: &ActorId,
        path: &str,
        content: &[u8],
        session_id: &EditSessionId,
    ) -> Result<String, CheckpointError> {
        let validated = crate::file_util::validate_workspace_relative_path(path)?;
        let storage = self.storage_ref()?;
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(actor)?;
        let threshold = self.full_snapshot_threshold;
        let snapshot_hex = if let Ok(text) = std::str::from_utf8(content) {
            layertwine::layered::agent::apply_agent_edit_full(
                storage,
                &agent_id,
                &validated,
                text,
                Some(*session_id),
                threshold,
            )
            .map_err(map_layertwine_error)?
            .to_hex()
        } else {
            // Binary edits bypass the session-tagged text path; record them
            // verbatim then associate the snapshot with the session below.
            self.apply_agent_edit(actor, &validated, content)?
        };
        // Associate the produced delta (when any) and snapshot with the session.
        if let Some(snapshot_id) = layertwine::core::types::ContentId::from_hex(&snapshot_hex) {
            if let Ok(snapshot) = storage.get_snapshot(&snapshot_id) {
                if let Some(delta_id) = snapshot.deltas.last() {
                    let _ = storage.append_delta_to_session(session_id, delta_id);
                }
                let _ = storage.associate_snapshot_with_session(session_id, &snapshot.id);
            }
        }
        Ok(snapshot_hex)
    }

    /// Record one file deletion for an actor inside an existing session.
    /// Returns the new snapshot id (hex).
    pub fn apply_agent_delete_in_session(
        &self,
        actor: &ActorId,
        path: &str,
        session_id: &EditSessionId,
    ) -> Result<String, CheckpointError> {
        let snapshot_hex = self.apply_agent_delete(actor, path)?;
        let storage = self.storage_ref()?;
        if let Some(snapshot_id) = layertwine::core::types::ContentId::from_hex(&snapshot_hex) {
            if let Ok(snapshot) = storage.get_snapshot(&snapshot_id) {
                if let Some(delta_id) = snapshot.deltas.last() {
                    let _ = storage.append_delta_to_session(session_id, delta_id);
                }
                let _ = storage.associate_snapshot_with_session(session_id, &snapshot.id);
            }
        }
        Ok(snapshot_hex)
    }

    /// Roll back an entire session on an actor's partition: the partition
    /// pointer moves to the state before the session started (deletions,
    /// full snapshots and ordinary deltas are all covered via the session
    /// manifest). Returns the snapshot id rolled back to (hex).
    pub fn rollback_session(
        &self,
        entity_id: &str,
        session_id: &EditSessionId,
    ) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        let pid = layertwine::layered::agent::agent_partition_id(&agent_id);
        layertwine::layered::transition::rollback_session(storage, &pid, session_id)
            .map(|id| id.to_hex())
            .map_err(map_layertwine_error)
    }

    // ── undo / redo (partition cursor) ──────────────────────────────────

    /// Undo the last edit on an actor's partition (pushes onto the redo
    /// stack). Returns the snapshot id moved back to (hex).
    pub fn undo_edit(&self, entity_id: &str) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        let pid = layertwine::layered::agent::agent_partition_id(&agent_id);
        layertwine::layered::transition::rollback_partition_with_redo(storage, &pid)
            .map(|id| id.to_hex())
            .map_err(map_layertwine_error)
    }

    /// Redo the most recently undone edit on an actor's partition.
    /// Returns the restored snapshot id (hex).
    pub fn redo_edit(&self, entity_id: &str) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        let pid = layertwine::layered::agent::agent_partition_id(&agent_id);
        layertwine::layered::transition::redo_partition(storage, &pid)
            .map(|id| id.to_hex())
            .map_err(map_layertwine_error)
    }

    /// Whether a redo is available on an actor's partition.
    pub fn can_redo(&self, entity_id: &str) -> Result<bool, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        let pid = layertwine::layered::agent::agent_partition_id(&agent_id);
        let partition = storage
            .get_partition(&pid)
            .map_err(map_layertwine_error)?;
        Ok(!partition.redo_stack.is_empty())
    }
}
