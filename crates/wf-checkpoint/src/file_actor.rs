use std::path::PathBuf;
use std::sync::Arc;

use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::{Snapshot, SnapshotContent};
use layertwine::layered::agent;
use layertwine::storage::repository::{PartitionStore, SnapshotStore};
use wf_types::config::file_checkpoint::FailureBehavior;

use crate::actor_id::{ActorId, ActorKind};
use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::file::FileCheckpointManager;
use crate::file_util::{map_layertwine_error, seed_initial_snapshot, sha256_hex};
use crate::provenance::DeltaSummary;
use crate::recent_agent_writes::RecentAgentWrites;
use crate::script_capture::{CollectedChange, CollectedChangeKind};

use std::collections::HashSet;

impl FileCheckpointManager {
    /// Resolve the actor partition for a child execution: full `ActorId`
    /// strings parse as-is; otherwise a child actor is derived from the
    /// given parent (`parent.child(execution_id)`) so nested executions are
    /// isolated in their own partition.
    pub fn actor_id_for_child(
        &self,
        entity_id: &str,
        parent: Option<&ActorId>,
    ) -> Result<ActorId, CheckpointError> {
        if let Some(actor) = self.actor_index.get(entity_id) {
            return Ok(actor.clone());
        }
        if let Ok(actor) = ActorId::parse(entity_id) {
            self.actor_index
                .insert(entity_id.to_string(), actor.clone());
            return Ok(actor);
        }
        let child_id = wf_types::Id::from(entity_id.to_string());
        let actor = match parent {
            Some(parent) => parent
                .child(&child_id)
                .map_err(|e| CheckpointError::Validation {
                    reason: format!("invalid child actor for '{entity_id}': {e}"),
                }),
            None => ActorId::new(ActorKind::Agent, &[child_id]).map_err(|e| {
                CheckpointError::Validation {
                    reason: format!("invalid actor for '{entity_id}': {e}"),
                }
            }),
        }?;
        self.actor_index
            .insert(entity_id.to_string(), actor.clone());
        Ok(actor)
    }

    /// Resolve the actor for an execution entity id. Full `ActorId` strings
    /// (e.g. `agent:{loop_id}` / `wf:{workflow_id}/child:{subgraph_id}`)
    /// parse as-is; bare execution ids map to a root agent actor. A
    /// previously resolved hierarchical actor (via [`Self::resolve_actor`])
    /// takes precedence so later calls keep the same partition.
    pub fn actor_id_for(&self, entity_id: &str) -> ActorId {
        if let Some(actor) = self.actor_index.get(entity_id) {
            return actor.clone();
        }
        let actor = match ActorId::parse(entity_id) {
            Ok(actor) => actor,
            Err(_) => crate::file_util::root_actor(wf_types::Id::from(entity_id.to_string())),
        };
        self.actor_index
            .insert(entity_id.to_string(), actor.clone());
        actor
    }

    /// Resolve the actor for an execution with an optional immediate parent
    /// (sub-execution isolation). A child execution whose parent is
    /// already in the actor index is encoded as `parent.child(execution_id)`
    /// (`{kind}:{parent}/child:{child}`), keeping nested executions in their
    /// own hierarchical partition. Falls back to a root actor when the
    /// parent is unknown.
    pub fn resolve_actor(&self, entity_id: &str, parent_execution_id: Option<&str>) -> ActorId {
        if let Some(actor) = self.actor_index.get(entity_id) {
            return actor.clone();
        }
        if let Ok(actor) = ActorId::parse(entity_id) {
            self.actor_index
                .insert(entity_id.to_string(), actor.clone());
            return actor;
        }
        let child_id = wf_types::Id::from(entity_id.to_string());
        let actor = match parent_execution_id {
            Some(parent) if parent != entity_id => match self.actor_index.get(parent) {
                Some(parent_actor) => parent_actor
                    .child(&child_id)
                    .unwrap_or_else(|_| crate::file_util::root_actor(child_id.clone())),
                None => crate::file_util::root_actor(child_id.clone()),
            },
            _ => crate::file_util::root_actor(child_id.clone()),
        };
        self.actor_index
            .insert(entity_id.to_string(), actor.clone());
        actor
    }

    /// The resolved actor of an entity, if it was resolved earlier.
    pub fn resolved_actor(&self, entity_id: &str) -> Option<ActorId> {
        self.actor_index.get(entity_id).map(|a| a.clone())
    }

    // ── actor partition primitives ──────────────────────────────────

    /// Ensure the actor's agent partition exists, seeding it with an empty
    /// baseline snapshot on first use.
    pub fn ensure_agent_partition(&self, actor: &ActorId) -> Result<(), CheckpointError> {
        let storage = self.storage_ref()?;
        let agent_id = actor.to_agent_instance_id();
        let pid = agent::agent_partition_id(&agent_id);
        if storage.get_partition(&pid).is_ok() {
            return Ok(());
        }
        let initial = seed_initial_snapshot(storage, &agent_id)?;
        agent::ensure_agent_partition(storage, &agent_id, initial).map_err(map_layertwine_error)?;
        Ok(())
    }

    /// Record one file edit for an actor: text goes through layertwine's
    /// line-diff `apply_agent_edit`; binary content is snapshotted verbatim
    /// via `SnapshotContent::FileContent` (no line diff). Returns the new
    /// snapshot id (hex).
    pub fn apply_agent_edit(
        &self,
        actor: &ActorId,
        path: &str,
        content: &[u8],
    ) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(actor)?;
        let snapshot_id = if let Ok(text) = std::str::from_utf8(content) {
            agent::apply_agent_edit(storage, &agent_id, path, text).map_err(map_layertwine_error)?
        } else {
            let file_node = FileNode::new(PathBuf::from(path), content);
            let snapshot = Snapshot::new_with_content(
                file_node,
                SnapshotContent::FileContent(content.to_vec()),
                format!("file://{}", path),
                format!("agent/{}", agent_id),
                vec![],
                vec![],
            );
            storage
                .store_snapshot(&snapshot, content)
                .map_err(map_layertwine_error)?;
            let pid = agent::agent_partition_id(&agent_id);
            storage
                .update_pointer(&pid, &snapshot.id)
                .map_err(map_layertwine_error)?;
            snapshot.id
        };
        // Clear any earlier deletion marker for this path (the file exists
        // again); register the write for the manual watcher. The registry is
        // keyed by both the (workspace-relative) edit path and the absolute
        // path — watcher events always carry absolute paths.
        if !content.is_empty() {
            if let Some(mut deleted) = self.deleted_files.get_mut(actor.as_str()) {
                deleted.remove(path);
            }
        }
        let write_hash = sha256_hex(content);
        self.recent_agent_writes
            .register(PathBuf::from(path), write_hash.clone());
        if let Some(root) = &self.workspace_root {
            self.recent_agent_writes
                .register(root.join(path), write_hash.clone());
        }
        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::file_changed_with_summary(
                snapshot_id.to_hex(),
                path,
                actor.as_str(),
                Some(DeltaSummary {
                    file: path.to_string(),
                    source: actor.as_str().to_string(),
                    timestamp: wf_common::now(),
                    snapshot_id: snapshot_id.to_hex(),
                    hash: write_hash.clone(),
                }),
            ));
        }
        Ok(snapshot_id.to_hex())
    }

    /// Record one file deletion for an actor (explicit deletion semantics):
    /// advance the layertwine agent partition with a
    /// `SnapshotContent::Deleted`-marked snapshot, register the deletion
    /// projection marker, and notify the manual watcher (the resulting
    /// filesystem event is the agent's own write and must be skipped).
    /// Returns the new snapshot id (hex).
    pub fn apply_agent_delete(
        &self,
        actor: &ActorId,
        path: &str,
    ) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(actor)?;
        let snapshot_id =
            agent::apply_agent_delete(storage, &agent_id, path).map_err(map_layertwine_error)?;
        self.deleted_files
            .entry(actor.as_str().to_string())
            .or_default()
            .insert(path.to_string());
        let write_hash = sha256_hex(b"");
        self.recent_agent_writes
            .register(PathBuf::from(path), write_hash.clone());
        if let Some(root) = &self.workspace_root {
            self.recent_agent_writes
                .register(root.join(path), write_hash.clone());
        }
        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::file_changed_with_summary(
                snapshot_id.to_hex(),
                path,
                actor.as_str(),
                Some(DeltaSummary {
                    file: path.to_string(),
                    source: actor.as_str().to_string(),
                    timestamp: wf_common::now(),
                    snapshot_id: snapshot_id.to_hex(),
                    hash: write_hash.clone(),
                }),
            ));
        }
        Ok(snapshot_id.to_hex())
    }

    /// Record a manual (human/IDE) edit into the global manual partition,
    /// bypassing any actor. Text content goes through layertwine's line-diff
    /// `apply_manual_edit`; binary content is snapshotted verbatim via
    /// `SnapshotContent::FileContent`. Returns the new snapshot id (hex).
    pub fn apply_manual_edit(&self, path: &str, content: &[u8]) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let manual_pid = layertwine::layered::manual::manual_partition_id();
        if storage.get_partition(&manual_pid).is_err() {
            let seed = seed_initial_snapshot(
                storage,
                &layertwine::core::types::AgentInstanceId("manual".into()),
            )?;
            layertwine::layered::manual::ensure_manual_partition(storage, seed)
                .map_err(map_layertwine_error)?;
        }
        let snapshot_id = if let Ok(text) = std::str::from_utf8(content) {
            layertwine::layered::manual::apply_manual_edit(storage, path, text)
                .map_err(map_layertwine_error)?
        } else {
            let file_node = FileNode::new(PathBuf::from(path), content);
            let snapshot = Snapshot::new_with_content(
                file_node,
                SnapshotContent::FileContent(content.to_vec()),
                format!("file://{}", path),
                "manual".to_string(),
                vec![],
                vec![],
            );
            storage
                .store_snapshot(&snapshot, content)
                .map_err(map_layertwine_error)?;
            storage
                .update_pointer(
                    &layertwine::layered::manual::manual_partition_id(),
                    &snapshot.id,
                )
                .map_err(map_layertwine_error)?;
            snapshot.id
        };
        if let Some(ref bus) = self.event_bus {
            let write_hash = sha256_hex(content);
            bus.publish(CheckpointEventBus::file_changed_with_summary(
                snapshot_id.to_hex(),
                path,
                "manual",
                Some(DeltaSummary {
                    file: path.to_string(),
                    source: "manual".to_string(),
                    timestamp: wf_common::now(),
                    snapshot_id: snapshot_id.to_hex(),
                    hash: write_hash,
                }),
            ));
        }
        Ok(snapshot_id.to_hex())
    }

    /// Record a manual (human/IDE) file deletion into the global manual
    /// partition (explicit deletion semantics via
    /// `SnapshotContent::Deleted`). Returns the new snapshot id (hex).
    pub fn apply_manual_delete(&self, path: &str) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let manual_pid = layertwine::layered::manual::manual_partition_id();
        if storage.get_partition(&manual_pid).is_err() {
            let seed = seed_initial_snapshot(
                storage,
                &layertwine::core::types::AgentInstanceId("manual".into()),
            )?;
            layertwine::layered::manual::ensure_manual_partition(storage, seed)
                .map_err(map_layertwine_error)?;
        }
        let snapshot_id = layertwine::layered::manual::apply_manual_delete(storage, path)
            .map_err(map_layertwine_error)?;
        if let Some(ref bus) = self.event_bus {
            let write_hash = sha256_hex(b"");
            bus.publish(CheckpointEventBus::file_changed_with_summary(
                snapshot_id.to_hex(),
                path,
                "manual",
                Some(DeltaSummary {
                    file: path.to_string(),
                    source: "manual".to_string(),
                    timestamp: wf_common::now(),
                    snapshot_id: snapshot_id.to_hex(),
                    hash: write_hash,
                }),
            ));
        }
        Ok(snapshot_id.to_hex())
    }

    /// Apply a set of collected workspace changes (script capture) as agent
    /// edits on the actor partition. Add/Modify changes read the file
    /// content from disk; Delete changes record the explicit deletion
    /// (marker + projection). Per-file failures follow `behavior`.
    /// Returns the number of successfully applied changes.
    pub fn apply_workspace_changes(
        &self,
        actor: &ActorId,
        base_dir: &std::path::Path,
        changes: &[CollectedChange],
        behavior: FailureBehavior,
    ) -> Result<usize, CheckpointError> {
        let mut applied = 0;
        for change in changes {
            let Ok(relative) = change.path.strip_prefix(base_dir) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            match change.kind {
                CollectedChangeKind::Delete => match self.apply_agent_delete(actor, &relative) {
                    Ok(_) => applied += 1,
                    Err(err) => match behavior {
                        FailureBehavior::Error => return Err(err),
                        FailureBehavior::Warn => {
                            tracing::warn!("failed to apply delete of '{relative}': {err}")
                        }
                        FailureBehavior::Ignore => {}
                    },
                },
                CollectedChangeKind::Add | CollectedChangeKind::Modify => {
                    let content = match std::fs::read(&change.path) {
                        Ok(content) => content,
                        Err(err) => match behavior {
                            FailureBehavior::Error => {
                                return Err(CheckpointError::Io(std::io::Error::other(format!(
                                    "failed to read changed file '{relative}': {err}"
                                ))));
                            }
                            FailureBehavior::Warn => {
                                tracing::warn!("failed to read changed file '{relative}': {err}");
                                continue;
                            }
                            FailureBehavior::Ignore => continue,
                        },
                    };
                    match self.apply_agent_edit(actor, &relative, &content) {
                        Ok(_) => applied += 1,
                        Err(err) => match behavior {
                            FailureBehavior::Error => return Err(err),
                            FailureBehavior::Warn => {
                                tracing::warn!("failed to apply edit of '{relative}': {err}")
                            }
                            FailureBehavior::Ignore => {}
                        },
                    }
                }
            }
        }
        Ok(applied)
    }

    /// Discard an execution's file changes: revert the actor partition
    /// pointer to its parent (best-effort), delete the actor partition
    /// entirely (partition + history rows), and drop the in-memory
    /// projection index. Snapshots/deltas remain as immutable history
    /// (INSERT-ONLY, GC'd separately). No-op when the actor has no
    /// partition.
    pub fn discard_execution(&self, entity_id: &str) -> Result<(), CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        let pid = agent::agent_partition_id(&agent_id);
        if storage.get_partition(&pid).is_err() {
            return Ok(());
        }
        // Best-effort pointer revert; there is nothing to revert when the
        // partition has no parent (the seed snapshot), and the partition is
        // deleted right after anyway.
        let _ = agent::discard_agent_edit(storage, &agent_id);
        storage
            .delete_partition(&pid)
            .map_err(map_layertwine_error)?;
        self.latest_checkpoints.remove(actor.as_str());
        self.deleted_files.remove(actor.as_str());
        Ok(())
    }

    /// Shared registry of recent agent writes (the manual watcher reads it).
    pub fn recent_agent_writes(&self) -> &Arc<RecentAgentWrites> {
        &self.recent_agent_writes
    }

    /// File paths currently marked deleted for the actor (keyed by the
    /// actor id string, e.g. `checkpoint.metadata.author`).
    pub fn deleted_files(&self, author: &str) -> HashSet<String> {
        self.deleted_files
            .get(author)
            .map(|set| set.clone())
            .unwrap_or_default()
    }
}
