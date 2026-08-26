use layertwine::checkpoint::types::{Checkpoint, CheckpointMetadata};
use layertwine::core::types::{AgentInstanceId, CheckpointId};
use layertwine::layered::agent;
use layertwine::storage::repository::{CheckpointPersist, PartitionStore};
use layertwine::storage::sqlite::SqliteStorage;

use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::file::FileCheckpointManager;
use crate::file_util::{map_layertwine_error, seed_initial_snapshot};

/// Result of a merge commit: the layertwine merge outcome plus the
/// multi-parent checkpoint id created to record the merge in the DAG.
#[derive(Debug, Clone)]
pub struct MergeCommitResult {
    pub merge_result: layertwine::layered::MergeResult,
    pub checkpoint_id: String,
}

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
    /// per feature, sequential accumulation), then create a multi-parent
    /// merge commit checkpoint.
    pub fn merge_features_to_staged(
        &self,
        feature_names: &[&str],
    ) -> Result<MergeCommitResult, CheckpointError> {
        let storage = self.storage_ref()?;
        self.ensure_staged_ready(storage)?;

        let staged_cp = self.latest_staged_checkpoint_id(storage)?;
        let feature_cps: Vec<Option<String>> = feature_names
            .iter()
            .map(|name| self.latest_feature_checkpoint_id(storage, name))
            .collect::<Result<_, _>>()?;

        let names: Vec<String> = feature_names.iter().map(|s| s.to_string()).collect();
        let ws = self.workspace_key();
        let merge_result = layertwine::layered::staged::merge_features_to_staged(
            storage,
            &names,
            ws.as_deref(),
        )
        .map_err(map_layertwine_error)?;

        let mut parents: Vec<CheckpointId> = Vec::new();
        for id_str in std::iter::once(&staged_cp).chain(feature_cps.iter()).flatten() {
            if let Some(cid) = CheckpointId::from_hex(id_str) {
                if !parents.contains(&cid) {
                    parents.push(cid);
                }
            }
        }

        let snapshot_ids = vec![merge_result.snapshot_id];
        let checkpoint = Checkpoint::new(
            snapshot_ids,
            parents,
            CheckpointMetadata::new(
                "staged",
                &format!("merge {} features into staged", feature_names.len()),
            ),
        );
        storage
            .store_checkpoint(&checkpoint)
            .map_err(map_layertwine_error)?;

        Ok(MergeCommitResult {
            merge_result,
            checkpoint_id: checkpoint.id.to_hex(),
        })
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
    ) -> Result<MergeCommitResult, CheckpointError> {
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
    /// `CheckpointRepo` from the attached Sqlite storage, runs the sweep,
    /// and publishes a `GcCompleted` event with the statistics. Returns
    /// the `GcStats` (removed checkpoints / snapshots / freed bytes).
    pub fn run_gc(
        &self,
        retention: layertwine::git_sync::GcRetention,
    ) -> Result<layertwine::git_sync::GcStats, CheckpointError> {
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

    // ── merge commit helpers ──────────────────────────────────────────

    /// Find the latest checkpoint id for a feature (integrated) partition
    /// by scanning stored checkpoints whose author matches the feature name.
    pub(crate) fn latest_feature_checkpoint_id(
        &self,
        storage: &SqliteStorage,
        feature_name: &str,
    ) -> Result<Option<String>, CheckpointError> {
        let checkpoints = storage.list_checkpoints().map_err(map_layertwine_error)?;
        let latest = checkpoints
            .iter()
            .filter(|c| c.metadata.author == feature_name)
            .max_by_key(|c| c.created_at);
        Ok(latest.map(|c| c.id.to_hex()))
    }

    /// Find the latest checkpoint id for the staged partition by scanning
    /// stored checkpoints whose author is "staged".
    pub(crate) fn latest_staged_checkpoint_id(
        &self,
        storage: &SqliteStorage,
    ) -> Result<Option<String>, CheckpointError> {
        let checkpoints = storage.list_checkpoints().map_err(map_layertwine_error)?;
        let latest = checkpoints
            .iter()
            .filter(|c| c.metadata.author == "staged")
            .max_by_key(|c| c.created_at);
        Ok(latest.map(|c| c.id.to_hex()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::FileContentEntry;
    use std::collections::HashSet;
    use std::time::Duration;

    fn manager() -> FileCheckpointManager {
        FileCheckpointManager::new_in_memory().unwrap()
    }

    fn entry(path: &str, content: &[u8]) -> FileContentEntry {
        FileContentEntry::new(path, content.to_vec())
    }

    fn stored(storage: &SqliteStorage, id_hex: &str) -> Checkpoint {
        storage
            .get_checkpoint(&CheckpointId::from_hex(id_hex).unwrap())
            .unwrap()
    }

    fn parent_ids(checkpoint: &Checkpoint) -> HashSet<String> {
        checkpoint.parents.iter().map(|p| p.to_hex()).collect()
    }

    /// Merge one actor's changes into a fresh feature and record the
    /// feature-head checkpoint (authored by the feature name, chained onto
    /// the merge commit).
    fn make_feature(
        manager: &FileCheckpointManager,
        exec: &str,
        path: &str,
        feature: &str,
    ) -> (String, String) {
        manager
            .create_checkpoint(exec, &[entry(path, b"base")])
            .unwrap();
        manager
            .create_checkpoint(exec, &[entry(path, b"edit")])
            .unwrap();
        let merged = manager.merge_entity_changes(exec, feature).unwrap();
        let commit_id = stored(manager.storage().unwrap(), &merged.checkpoint_id)
            .id
            .to_hex();
        let storage = manager.storage().unwrap();
        let cp = Checkpoint::new(
            vec![merged.merge_result.snapshot_id],
            vec![CheckpointId::from_hex(&commit_id).unwrap()],
            CheckpointMetadata::new(feature, "feature head"),
        );
        storage.store_checkpoint(&cp).unwrap();
        (cp.id.to_hex(), merged.checkpoint_id)
    }

    /// Distinct `created_at` stamps so "latest by author" lookups are
    /// deterministic across same-millisecond writes.
    fn tick() {
        std::thread::sleep(Duration::from_millis(2));
    }

    #[test]
    fn merge_features_to_staged_creates_multi_parent() {
        let manager = manager();
        let (feature_a, _ma) = make_feature(&manager, "exec-a", "a.txt", "feature-a");
        tick();
        let (feature_b, _mb) = make_feature(&manager, "exec-b", "b.txt", "feature-b");
        tick();
        let (feature_c, _mc) = make_feature(&manager, "exec-c", "c.txt", "feature-c");

        // Round 1 joins two features: no staged commit exists yet, so the
        // parents are exactly the two feature heads.
        let round1 = manager
            .merge_features_to_staged(&["feature-a", "feature-b"])
            .unwrap();
        let storage = manager.storage().unwrap();
        assert_eq!(
            parent_ids(&stored(storage, &round1.checkpoint_id)),
            HashSet::from([feature_a.clone(), feature_b.clone()]),
            "staged join must link every participating feature head"
        );

        // Round 2 chains onto the previous staged commit and adds the new
        // feature head.
        tick();
        let round2 = manager.merge_features_to_staged(&["feature-c"]).unwrap();
        assert_eq!(
            parent_ids(&stored(storage, &round2.checkpoint_id)),
            HashSet::from([round1.checkpoint_id.clone(), feature_c]),
        );
    }

    #[test]
    fn checkpoint_dag_traversal_reaches_parents() {
        let manager = manager();
        let (feature_a, merge_a) = make_feature(&manager, "exec-a", "a.txt", "feature-a");
        tick();
        let (feature_b, merge_b) = make_feature(&manager, "exec-b", "b.txt", "feature-b");

        let joined = manager
            .merge_features_to_staged(&["feature-a", "feature-b"])
            .unwrap();

        // Walk the DAG from the staged merge commit through `parents` edges.
        let storage = manager.storage().unwrap();
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue = vec![joined.checkpoint_id.clone()];
        while let Some(id) = queue.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            for parent in &stored(storage, &id).parents {
                queue.push(parent.to_hex());
            }
        }

        // Every participant of the merges is reachable from the join commit.
        for expected in [
            joined.checkpoint_id.clone(),
            feature_a,
            feature_b,
            merge_a,
            merge_b,
        ] {
            assert!(
                seen.contains(&expected),
                "checkpoint {expected} must be reachable from the merge commit"
            );
        }
    }
}
