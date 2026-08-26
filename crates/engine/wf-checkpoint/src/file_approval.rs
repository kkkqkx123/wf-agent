use std::path::{Path, PathBuf};

use layertwine::checkpoint::types::{Checkpoint, CheckpointMetadata};
use layertwine::core::file_node::FileNode;
use layertwine::core::partition::Partition;
use layertwine::core::snapshot::{Snapshot, SnapshotContent};
use layertwine::core::types::{AgentInstanceId, CheckpointId, SnapshotId};
use layertwine::storage::repository::{
    CheckpointPersist, FileNodeStore, PartitionStore, SnapshotStore,
};
use layertwine::storage::sqlite::SqliteStorage;
use wf_types::config::file_checkpoint::ConflictBehavior;

use crate::approval::{inject_conflict_markers, to_conflict_views, MergeOutcome, PendingApproval};
use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::file::FileCheckpointManager;
use crate::file_merge::MergeCommitResult;
use crate::file_util::{map_layertwine_error, resolve_restore_target, sha256_hex};
use crate::provenance::DeltaSummary;

impl FileCheckpointManager {
    // ── approval layer (list / approve / reject) ─────────────────────

    /// All pending approvals: actor partitions at the approval layer with
    /// more than one history entry (submitted but neither merged nor
    /// rejected). Persisted in Sqlite, so pending approvals survive across
    /// executions ("review after the run ends").
    pub fn list_pending_approvals(&self) -> Result<Vec<PendingApproval>, CheckpointError> {
        let storage = self.storage_ref()?;
        let pending = layertwine::layered::approval::list_pending_approvals(storage)
            .map_err(map_layertwine_error)?;
        let mut views = Vec::with_capacity(pending.len());
        for partition in pending {
            let actor = match &partition.partition_type {
                layertwine::core::types::PartitionType::Approval(id) => id.0.clone(),
                other => other.name(),
            };
            let last_id =
                partition
                    .history
                    .last()
                    .copied()
                    .ok_or_else(|| CheckpointError::Corrupted {
                        id: partition.id.to_string(),
                        reason: "pending approval partition has empty history".to_string(),
                    })?;
            let last_snapshot = storage
                .get_snapshot(&last_id)
                .map_err(map_layertwine_error)?;
            let mut changes = Vec::new();
            for snapshot_id in &partition.history {
                let snapshot = storage
                    .get_snapshot(snapshot_id)
                    .map_err(map_layertwine_error)?;
                if let Some(summary) =
                    crate::provenance::DeltaSummary::from_snapshot(storage, &snapshot)?
                {
                    changes.push(summary);
                }
            }
            views.push(PendingApproval {
                actor,
                snapshot_id: last_id.to_hex(),
                submitted_at: last_snapshot.created_at,
                changes,
            });
        }
        views.sort_by_key(|a| a.submitted_at);
        Ok(views)
    }

    /// Reject a pending approval: roll the actor's approval partition back to
    /// its baseline. Returns the baseline snapshot id (hex).
    pub fn reject_changes(&self, entity_id: &str) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        let baseline = layertwine::layered::approval::reject_approval(storage, &agent_id)
            .map_err(map_layertwine_error)?;
        Ok(baseline.to_hex())
    }

    /// Approve an actor's pending changes: merge them into the named feature
    /// and apply the configured conflict behavior.
    ///
    /// `paths` selects the file-level approval mode:
    /// - `None`: approve the whole submission (full batch, today's behavior).
    /// - `Some(paths)`: advance only the listed files into the feature
    ///   partition (content taken verbatim from the approval partition — no
    ///   three-way merge, the baselines are identical); every approved file
    ///   publishes a `FileChanged` event and the remaining files stay
    ///   pending in the approval layer.
    ///
    /// - `ConflictBehavior::Marker` (default): merged text keeps conflict
    ///   markers embedded in the affected files (written to `workspace_root`
    ///   when provided) and the outcome reports them; execution continues.
    /// - `ConflictBehavior::Fail`: any conflict aborts with an error.
    /// - `ConflictBehavior::Approval`: conflicts stay pending in the
    ///   approval layer instead of being merged.
    pub fn approve_changes(
        &self,
        entity_id: &str,
        feature_name: &str,
        paths: Option<Vec<String>>,
        conflict_behavior: ConflictBehavior,
        workspace_root: Option<&Path>,
    ) -> Result<MergeOutcome, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(&actor)?;
        self.ensure_approval_ready(storage, &agent_id)?;

        let submitted = self.move_agent_to_approval(entity_id)?;

        // File-level approval: advance only the selected files into the
        // feature partition, leaving the others pending. The approval
        // partition is NOT reset here, so `list_pending_approvals` keeps
        // reporting the remaining files until the host approves or rejects
        // them explicitly.
        if let Some(paths) = paths {
            if paths.is_empty() {
                return Ok(MergeOutcome {
                    merged: false,
                    snapshot_id: submitted,
                    conflicts: vec![],
                    conflict_files: vec![],
                    message: "no paths selected; changes remain pending".to_string(),
                });
            }
            let approval_pid =
                layertwine::layered::approval::approval_agent_partition_id(&agent_id);
            let approval_partition = storage
                .get_partition(&approval_pid)
                .map_err(map_layertwine_error)?;
            let baseline = approval_partition.history.first().copied().ok_or_else(|| {
                CheckpointError::Corrupted {
                    id: approval_pid.to_string(),
                    reason: "approval partition has empty history".to_string(),
                }
            })?;
            let mut approved: Vec<String> = Vec::new();
            let mut last_snapshot = submitted;
            for path in &paths {
                let Some(content) = self.approval_file_content(storage, &agent_id, path)? else {
                    continue;
                };
                let snap =
                    self.apply_feature_edit(storage, feature_name, path, &content, baseline)?;
                if let Some(ref bus) = self.event_bus {
                    bus.publish(CheckpointEventBus::file_changed_with_summary(
                        snap.clone(),
                        path,
                        actor.as_str(),
                        Some(DeltaSummary {
                            file: path.clone(),
                            source: actor.as_str().to_string(),
                            timestamp: wf_common::now(),
                            snapshot_id: snap.clone(),
                            hash: sha256_hex(&content),
                        }),
                    ));
                }
                last_snapshot = snap;
                approved.push(path.clone());
            }
            return Ok(MergeOutcome {
                merged: !approved.is_empty(),
                snapshot_id: last_snapshot,
                conflicts: vec![],
                conflict_files: vec![],
                message: if approved.is_empty() {
                    "no approved files; changes remain pending".to_string()
                } else {
                    format!(
                        "approved {} file(s) ({}); others remain pending",
                        approved.len(),
                        approved.join(", ")
                    )
                },
            });
        }

        if conflict_behavior == ConflictBehavior::Approval {
            // Route conflicted changes to the approval layer: keep the
            // submission pending and let the host resolve it.
            return Ok(MergeOutcome {
                merged: false,
                snapshot_id: submitted,
                conflicts: vec![],
                conflict_files: vec![],
                message: "changes remain pending in the approval layer".to_string(),
            });
        }

        let merged = self.merge_agent_to_feature(entity_id, feature_name)?;
        let merged_snapshot = storage
            .get_snapshot(&merged.snapshot_id)
            .map_err(map_layertwine_error)?;
        let file = crate::provenance::snapshot_file_path(storage, &merged_snapshot)?;
        let conflicts = to_conflict_views(&file, &merged.conflicts);
        let mut conflict_files: Vec<String> = conflicts
            .iter()
            .map(|c| c.file.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        conflict_files.sort();

        if !conflicts.is_empty() && conflict_behavior == ConflictBehavior::Fail {
            return Err(CheckpointError::MergeConflict {
                actor: actor.as_str().to_string(),
                files: conflict_files,
            });
        }

        if !conflicts.is_empty() && conflict_behavior == ConflictBehavior::Marker {
            if let Some(root) = workspace_root {
                let merged_text =
                    layertwine::layered::transition::reconstruct_text(storage, &merged_snapshot)
                        .map_err(map_layertwine_error)?
                        .unwrap_or_default();
                let marked = inject_conflict_markers(&merged_text, &merged.conflicts);
                let target = resolve_restore_target(root, &file)?;
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, marked)?;
            }
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::merge_conflicted(
                    merged.snapshot_id.to_hex(),
                    conflict_files.clone(),
                    Some(actor.as_str().to_string()),
                ));
            }
        }

        Ok(MergeOutcome {
            merged: true,
            snapshot_id: merged.snapshot_id.to_hex(),
            conflicts,
            conflict_files,
            message: if merged.has_conflicts() {
                "merged with conflicts".to_string()
            } else {
                "merged cleanly".to_string()
            },
        })
    }

    /// Full merge entry point for an actor: move to approval, then merge
    /// into the named feature (the `ApprovalPolicy::auto` path).
    /// Creates a multi-parent merge commit checkpoint linking the feature
    /// and the actor's previous checkpoint as parents.
    pub fn merge_entity_changes(
        &self,
        entity_id: &str,
        feature_name: &str,
    ) -> Result<MergeCommitResult, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let feature_cp = self.latest_feature_checkpoint_id(storage, feature_name)?;
        let actor_cp = self.latest_checkpoint_id(storage, &actor)?;
        self.move_agent_to_approval(entity_id)?;
        let merge_result = self.merge_agent_to_feature(entity_id, feature_name)?;
        let mut parents: Vec<CheckpointId> = Vec::new();
        for id_str in [&feature_cp, &actor_cp].into_iter().flatten() {
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
            CheckpointMetadata::new(actor.as_str(), &format!("merge into {feature_name}")),
        );
        storage
            .store_checkpoint(&checkpoint)
            .map_err(map_layertwine_error)?;
        Ok(MergeCommitResult {
            merge_result,
            checkpoint_id: checkpoint.id.to_hex(),
        })
    }

    /// Latest recorded content of a path in an actor's approval partition
    /// (the submitted, not-yet-approved state). Falls back to the actor
    /// partition when the path has no approval entry (the layered merge
    /// advances one file at a time, so a multi-file submission only moves
    /// its last file into approval; the rest are read from the agent
    /// partition, which holds identical content). `None` when the path has
    /// no entry anywhere.
    pub(crate) fn approval_file_content(
        &self,
        storage: &SqliteStorage,
        agent_id: &AgentInstanceId,
        path: &str,
    ) -> Result<Option<Vec<u8>>, CheckpointError> {
        let latest_in = |partition: &Partition| -> Result<Option<Vec<u8>>, CheckpointError> {
            let mut latest: Option<Vec<u8>> = None;
            for snapshot_id in &partition.history {
                let snapshot = storage
                    .get_snapshot(snapshot_id)
                    .map_err(map_layertwine_error)?;
                let spath = crate::provenance::snapshot_file_path(storage, &snapshot)?;
                if spath != path {
                    continue;
                }
                if snapshot.is_deleted() {
                    latest = Some(Vec::new());
                    continue;
                }
                if let Some(content) = &snapshot.content {
                    latest = Some(content.to_bytes());
                } else {
                    let text =
                        layertwine::layered::transition::reconstruct_text(storage, &snapshot)
                            .map_err(map_layertwine_error)?
                            .unwrap_or_default();
                    latest = Some(text.into_bytes());
                }
            }
            Ok(latest)
        };

        let approval_pid = layertwine::layered::approval::approval_agent_partition_id(agent_id);
        if let Ok(partition) = storage.get_partition(&approval_pid) {
            if let Some(content) = latest_in(&partition)? {
                return Ok(Some(content));
            }
        }
        let agent_pid = layertwine::layered::agent::agent_partition_id(agent_id);
        let partition = storage
            .get_partition(&agent_pid)
            .map_err(map_layertwine_error)?;
        latest_in(&partition)
    }

    /// Advance a feature (integrated) partition directly with the given
    /// file content — no three-way merge, because for a file-level approval
    /// the approval baseline and the feature baseline are the same, so the
    /// content is authoritative. Keeps the last snapshot of the same path
    /// as a parent when one exists (provenance continuity).
    pub(crate) fn apply_feature_edit(
        &self,
        storage: &SqliteStorage,
        feature_name: &str,
        path: &str,
        content: &[u8],
        initial_snapshot_id: SnapshotId,
    ) -> Result<String, CheckpointError> {
        let pid = layertwine::layered::integrated::integrated_partition_id(feature_name);
        let partition = layertwine::layered::integrated::ensure_integrated_partition(
            storage,
            feature_name,
            initial_snapshot_id,
        )
        .map_err(map_layertwine_error)?;
        let parent = partition.history.iter().rev().find_map(|sid| {
            storage
                .get_snapshot(sid)
                .ok()
                .filter(|s| {
                    crate::provenance::snapshot_file_path(storage, s)
                        .map(|p| p == path)
                        .unwrap_or(false)
                })
                .map(|s| s.id)
        });
        let file_node = FileNode::new(PathBuf::from(path), content);
        let snapshot = Snapshot::new_with_content(
            file_node,
            SnapshotContent::FileContent(content.to_vec()),
            format!("file://{}", path),
            layertwine::core::types::PartitionType::Integrated(feature_name.to_string()).name(),
            parent.map_or_else(Vec::new, |p| vec![p]),
            vec![],
        );
        storage
            .store_file_node(&snapshot.file, content)
            .map_err(map_layertwine_error)?;
        storage
            .store_snapshot(&snapshot, content)
            .map_err(map_layertwine_error)?;
        storage
            .update_pointer(&pid, &snapshot.id)
            .map_err(map_layertwine_error)?;
        Ok(snapshot.id.to_hex())
    }

    /// Resolve merge conflicts for an entity by overwriting the conflicted
    /// files with the provided resolved content, clearing the conflict
    /// markers, then reporting how many conflicts remain.
    ///
    /// Each `(path, content)` pair is handled in two steps:
    /// 1. recorded as a fresh agent edit (overwrite), so the actor
    ///    partition reflects the resolution;
    /// 2. written directly into the feature (integrated) partition — no
    ///    three-way merge, because the resolution is authoritative — which
    ///    creates a clean snapshot without the `has_conflicts` flag and
    ///    clears the marker for that path.
    ///
    /// Returns the number of files that still carry an unresolved conflict
    /// after this operation (0 = fully resolved).
    pub fn resolve_conflicts(
        &self,
        entity_id: &str,
        feature_name: &str,
        resolutions: &[(String, Vec<u8>)],
    ) -> Result<usize, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(&actor)?;
        self.ensure_approval_ready(storage, &agent_id)?;

        let approval_pid = layertwine::layered::approval::approval_agent_partition_id(&agent_id);
        let approval_partition = storage
            .get_partition(&approval_pid)
            .map_err(map_layertwine_error)?;
        let baseline = approval_partition.history.first().copied().ok_or_else(|| {
            CheckpointError::Corrupted {
                id: approval_pid.to_string(),
                reason: "approval partition has empty history".to_string(),
            }
        })?;

        for (path, content) in resolutions {
            // Record the resolution as an agent edit (provenance) and
            // advance the feature partition with the resolved content,
            // dropping the conflict flag for the path.
            self.apply_agent_edit(&actor, path, content)?;
            self.apply_feature_edit(storage, feature_name, path, content, baseline)?;
        }

        // Remaining unresolved conflict count across staged/feature.
        Ok(crate::provenance::list_conflicts(storage, self.workspace_key().as_deref())?.len())
    }

    // ── end-of-execution approval policy ─────────────────────────────

    /// The configured layered approval policy.
    pub fn approval_policy(&self) -> crate::file::ApprovalPolicy {
        self.approval_policy
    }

    /// The configured merge conflict behavior.
    pub fn conflict_behavior(&self) -> ConflictBehavior {
        self.conflict_behavior
    }

    /// Default feature name merged into under `ApprovalPolicy::auto` (a
    /// per-execution feature keeps actors isolated while still landing the
    /// changes into the integrated layer).
    pub fn default_feature_name(entity_id: &str) -> String {
        format!("exec-{}", entity_id)
    }

    /// Agent loop end hook: applies the configured approval policy.
    ///
    /// - `None`: no-op (today's behavior; the actor partition keeps its
    ///   history and stays queryable).
    /// - `Auto`: move the actor to the approval layer and immediately merge
    ///   into the default feature partition.
    /// - `Llm` / `Manual`: move the actor to the approval layer and leave the
    ///   changes pending (`approve_changes` tool / host API resolve them,
    ///   possibly across executions).
    ///
    /// Best-effort by design: the file checkpoint layer must never break an
    /// execution that finished successfully, so failures are reported but
    /// not propagated.
    pub fn on_agent_complete(
        &self,
        entity_id: &str,
    ) -> Result<Option<layertwine::layered::MergeResult>, CheckpointError> {
        match self.approval_policy {
            crate::file::ApprovalPolicy::None => Ok(None),
            crate::file::ApprovalPolicy::Auto => {
                let feature = Self::default_feature_name(entity_id);
                let merged = self.merge_entity_changes(entity_id, &feature)?;
                Ok(Some(merged.merge_result))
            }
            crate::file::ApprovalPolicy::Llm | crate::file::ApprovalPolicy::Manual => {
                self.move_agent_to_approval(entity_id)?;
                Ok(None)
            }
        }
    }

    /// Approve a pending actor's changes using the configured conflict
    /// behavior (thin wrapper over [`Self::approve_changes`]).
    pub fn approve_pending(
        &self,
        entity_id: &str,
        feature_name: &str,
    ) -> Result<MergeOutcome, CheckpointError> {
        self.approve_changes(
            entity_id,
            feature_name,
            None,
            self.conflict_behavior,
            self.workspace_root.as_deref(),
        )
    }

    /// File-level approval: approve only the listed paths from a pending
    /// actor submission, leaving the rest pending in the approval layer
    /// (thin wrapper over [`Self::approve_changes`] with `paths`).
    pub fn approve_pending_paths(
        &self,
        entity_id: &str,
        feature_name: &str,
        paths: Vec<String>,
    ) -> Result<MergeOutcome, CheckpointError> {
        self.approve_changes(
            entity_id,
            feature_name,
            Some(paths),
            self.conflict_behavior,
            self.workspace_root.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::FileContentEntry;
    use std::collections::HashSet;

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

    /// Record a feature-head commit checkpoint (authored by the feature
    /// name), as produced by feature-level commit flows.
    fn seed_feature_checkpoint(
        storage: &SqliteStorage,
        feature: &str,
        snapshot: SnapshotId,
        parents: Vec<CheckpointId>,
    ) -> String {
        let cp = Checkpoint::new(
            vec![snapshot],
            parents,
            CheckpointMetadata::new(feature, "feature head"),
        );
        storage.store_checkpoint(&cp).unwrap();
        cp.id.to_hex()
    }

    #[test]
    fn merge_entity_changes_links_actor_history_without_feature_commit() {
        let manager = manager();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"base")])
            .unwrap();
        let latest = manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"edit")])
            .unwrap();

        let result = manager.merge_entity_changes("exec-1", "feature-1").unwrap();

        let storage = manager.storage().unwrap();
        let commit = stored(storage, &result.checkpoint_id);
        // No feature-head commit exists yet, so the only recorded parent is
        // the actor's previous checkpoint.
        assert_eq!(parent_ids(&commit), HashSet::from([latest.id]));
    }

    #[test]
    fn merge_entity_changes_creates_multi_parent() {
        let manager = manager();

        // First cycle establishes the actor history and the merge commit;
        // a feature-head checkpoint is then recorded for the feature.
        manager
            .create_checkpoint("exec-a", &[entry("a.txt", b"base")])
            .unwrap();
        let first = manager.merge_entity_changes("exec-a", "feature-1").unwrap();
        let storage = manager.storage().unwrap();
        let first_commit_id = stored(storage, &first.checkpoint_id).id.to_hex();
        let feature_cp = seed_feature_checkpoint(
            storage,
            "feature-1",
            first.merge_result.snapshot_id,
            vec![CheckpointId::from_hex(&first_commit_id).unwrap()],
        );

        // A second actor merges into the same feature: the new commit must
        // record both the feature head and the actor's own latest checkpoint.
        // Parallel contributors may textually conflict (resolution belongs
        // to the approval layer), yet the DAG commit records the merge with
        // all participants either way.
        manager
            .create_checkpoint("exec-b", &[entry("b.txt", b"base")])
            .unwrap();
        let actor_cp = manager
            .create_checkpoint("exec-b", &[entry("b.txt", b"edit")])
            .unwrap();
        let second = manager.merge_entity_changes("exec-b", "feature-1").unwrap();

        let commit = stored(storage, &second.checkpoint_id);
        assert_eq!(
            parent_ids(&commit),
            HashSet::from([feature_cp, actor_cp.id]),
            "merge commit must link every participant"
        );
    }
}
