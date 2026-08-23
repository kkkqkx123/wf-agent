use layertwine::core::types::AgentInstanceId;
use layertwine::layered::agent;
use layertwine::storage::repository::{CheckpointPersist, PartitionStore};
use layertwine::storage::sqlite::SqliteStorage;

use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::file::FileCheckpointManager;
use crate::file_util::{map_layertwine_error, seed_initial_snapshot};

impl FileCheckpointManager {
    // ── layered merge wrappers ────────────────────────────────────────

    pub(crate) fn ensure_approval_ready(
        &self,
        storage: &SqliteStorage,
        agent_id: &AgentInstanceId,
    ) -> Result<(), CheckpointError> {
        let pid = agent::agent_partition_id(agent_id);
        let partition = storage.get_partition(&pid).map_err(map_layertwine_error)?;
        let baseline =
            partition
                .history
                .first()
                .copied()
                .ok_or_else(|| CheckpointError::Corrupted {
                    id: pid.to_string(),
                    reason: "agent partition has empty history".to_string(),
                })?;
        layertwine::layered::approval::ensure_approval_agent_partition(storage, agent_id, baseline)
            .map_err(map_layertwine_error)?;
        Ok(())
    }

    pub(crate) fn ensure_staged_ready(
        &self,
        storage: &SqliteStorage,
    ) -> Result<(), CheckpointError> {
        let ws = self.workspace_key();
        let staged_pid = match ws.as_deref() {
            Some(key) => layertwine::layered::staged::staged_partition_id_for(key),
            None => layertwine::layered::staged::staged_partition_id(),
        };
        if storage.get_partition(&staged_pid).is_ok() {
            return Ok(());
        }
        let seed = seed_initial_snapshot(storage, &AgentInstanceId("staged".into()))?;
        layertwine::layered::staged::ensure_staged_partition(storage, seed, ws.as_deref())
            .map_err(map_layertwine_error)?;
        Ok(())
    }

    /// Move the actor's changes into the approval layer (three-way merge
    /// against the approval baseline). Returns the approval snapshot id.
    pub fn move_agent_to_approval(&self, entity_id: &str) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(&actor)?;
        self.ensure_approval_ready(storage, &agent_id)?;
        let snapshot_id =
            agent::move_agent_to_approval(storage, &agent_id).map_err(map_layertwine_error)?;
        Ok(snapshot_id.to_hex())
    }

    /// Merge the actor's approved changes into a feature (integrated)
    /// partition via three-way merge.
    pub fn merge_agent_to_feature(
        &self,
        entity_id: &str,
        feature_name: &str,
    ) -> Result<layertwine::layered::MergeResult, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(&actor)?;
        self.ensure_approval_ready(storage, &agent_id)?;
        layertwine::layered::integrated::merge_agent_to_feature(storage, &agent_id, feature_name)
            .map_err(map_layertwine_error)
    }

    /// Merge all given features into the staged partition (three-way merge
    /// per feature, sequential accumulation).
    pub fn merge_features_to_staged(
        &self,
        feature_names: &[&str],
    ) -> Result<layertwine::layered::MergeResult, CheckpointError> {
        let storage = self.storage_ref()?;
        self.ensure_staged_ready(storage)?;
        let names: Vec<String> = feature_names.iter().map(|s| s.to_string()).collect();
        let ws = self.workspace_key();
        layertwine::layered::staged::merge_features_to_staged(storage, &names, ws.as_deref())
            .map_err(map_layertwine_error)
    }

    /// Fork-join join step: merge each parallel feature branch into staged
    /// in order, then delete the branch head pointers.
    ///
    /// Each merge produces a multi-parent checkpoint (the DAG keeps the
    /// ancestry), and after the join the branch names are no longer needed
    /// as pointers — removing them does not affect provenance, which
    /// resolves through the checkpoint graph, not the branch registry.
    pub fn merge_branch_changes(
        &self,
        feature_names: &[&str],
    ) -> Result<layertwine::layered::MergeResult, CheckpointError> {
        let merged = self.merge_features_to_staged(feature_names)?;
        let storage = self.storage_ref()?;
        for name in feature_names {
            storage.delete_branch(name).map_err(map_layertwine_error)?;
        }
        Ok(merged)
    }

    /// Run physical garbage collection over the checkpoint repository
    /// (mark-sweep: branch heads + ancestors + git anchors + the most
    /// recent `retention.keep_recent_heads` partition heads are kept).
    ///
    /// Thin wrapper over [`layertwine::git_sync::gc::run_gc`]: loads the
    /// `CheckpointRepo` from the attached SQLite storage, runs the sweep,
    /// and publishes a `GcCompleted` event with the statistics. Returns
    /// the `GCStats` (removed checkpoints / snapshots / freed bytes).
    pub fn run_gc(
        &self,
        retention: layertwine::git_sync::GcRetention,
    ) -> Result<layertwine::git_sync::GCStats, CheckpointError> {
        let storage = self.storage_ref()?;
        let persist: Box<dyn layertwine::storage::repository::CheckpointPersist> =
            Box::new(storage.share());
        let mut repo = layertwine::checkpoint::repo::CheckpointRepo::load(persist)
            .map_err(map_layertwine_error)?;
        let stats =
            layertwine::git_sync::gc::run_gc(&mut repo, retention).map_err(map_layertwine_error)?;
        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::gc_completed(stats.clone()));
        }
        Ok(stats)
    }
}
