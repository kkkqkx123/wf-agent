//! Provenance queries over layertwine partitions.
//!
//! Queries are actor-partition centered and only use the layertwine
//! `Repository` traits (`PartitionStore` / `SnapshotStore` / `DeltaStore` /
//! `FileNodeStore`) — no direct SQL. Three query dimensions are supported:
//!
//! - by actor: `list_changes_by_actor` walks the actor partition history.
//! - by path: `list_changes_by_path` scans every partition's history.
//! - by time: window filters over `Delta.timestamp` / `Snapshot.created_at`.
//!
//! Plus workspace state (`get_actor_workspace`) and difference queries
//! (`diff_actors` /.

use std::collections::HashMap;

use layertwine::core::delta::Delta;
use layertwine::core::partition::Partition;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::{AgentInstanceId, PartitionType, SnapshotId, SourceType};
use layertwine::engine::merge::merge_texts;
use layertwine::storage::repository::{DeltaStore, PartitionStore, SnapshotStore};
use layertwine::storage::sqlite::SqliteStorage;

use crate::actor_id::ActorId;
use crate::approval::{to_conflict_views, ConflictView};
use crate::diff::DiffEngine;
use crate::error::CheckpointError;
use crate::file::FileContentEntry;
use crate::file_util::{map_layertwine_error, sha256_hex};

/// Seed path of the synthetic initial snapshot; excluded from provenance.
const SEED_PATH: &str = ".wf-checkpoint-seed";

/// Resolve the staged partition id for a workspace key (`None` = legacy
/// single-workspace fixed id).
pub(crate) fn staged_pid(workspace_key: Option<&str>) -> layertwine::core::types::PartitionId {
    match workspace_key {
        Some(key) => layertwine::layered::staged::staged_partition_id_for(key),
        None => layertwine::layered::staged::staged_partition_id(),
    }
}

/// One recorded change of a partition history entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeltaSummary {
    /// Relative file path.
    pub file: String,
    /// Origin: `agent:{actor}` / `manual` / `backup` (layertwine `SourceType`).
    pub source: String,
    /// Change time (Unix milliseconds).
    pub timestamp: i64,
    /// Snapshot id (hex).
    pub snapshot_id: String,
    /// Content hash (SHA-256 hex) of the resulting file bytes.
    pub hash: String,
}

/// Read view of a partition.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PartitionView {
    pub partition_id: String,
    pub name: String,
    /// `manual` | `agent` | `approval` | `integrated` | `unified` | `staged`.
    pub kind: String,
    /// Actor id for per-actor partitions (agent/approval), `None` otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Snapshot id (hex) of the partition pointer.
    pub current_snapshot: String,
    /// Number of history entries (INSERT-ONLY retention).
    pub history_len: usize,
    /// Creation time of the first history snapshot.
    pub created_at: i64,
    /// Time of the last history snapshot.
    pub updated_at: i64,
}

/// File content of an actor workspace at its current partition state
/// (`get_actor_workspace`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkspaceFile {
    pub path: String,
    pub content: Vec<u8>,
    pub hash: String,
    pub timestamp: i64,
}

/// Kind of a per-file difference between two workspace states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileDiffKind {
    Added,
    Modified,
    Deleted,
    Unchanged,
}

/// Per-file difference view (`diff_actors` /.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileDiffView {
    pub path: String,
    pub kind: FileDiffKind,
    /// Unified diff (text files only); `None` for binary content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletions: Option<usize>,
}

/// Whether a path matches the optional filter (plain substring match).
fn path_matches(path: &str, filter: Option<&str>) -> bool {
    match filter {
        Some(filter) if !filter.is_empty() => path.contains(filter),
        _ => true,
    }
}

/// The file path a snapshot applies to (chain head delta or own file node).
pub fn snapshot_file_path(
    storage: &SqliteStorage,
    snapshot: &Snapshot,
) -> Result<String, CheckpointError> {
    if let Some(delta_id) = snapshot.deltas.last() {
        let delta = storage.get_delta(delta_id).map_err(map_layertwine_error)?;
        Ok(delta.file.path_str().to_string())
    } else {
        Ok(snapshot.file.path_str().to_string())
    }
}

/// The delta of a snapshot chain (the last entry carries the edit that
/// produced the snapshot).
fn snapshot_last_delta(
    storage: &SqliteStorage,
    snapshot: &Snapshot,
) -> Result<Option<Delta>, CheckpointError> {
    let Some(delta_id) = snapshot.deltas.last() else {
        return Ok(None);
    };
    storage.get_delta(delta_id).map(Some).map_err(map_layertwine_error)
}

/// The byte content of a snapshot (verbatim content for binary, otherwise
/// line-diff reconstruction).
fn snapshot_content_bytes(
    storage: &SqliteStorage,
    snapshot: &Snapshot,
) -> Result<Vec<u8>, CheckpointError> {
    if let Some(content) = &snapshot.content {
        return Ok(content.to_bytes());
    }
    // A deleted snapshot has no content; treat it as empty bytes (the
    // deletion marker is consulted by the workspace/restore callers).
    Ok(
        layertwine::layered::transition::reconstruct_text(storage, snapshot)
            .map_err(map_layertwine_error)?
            .unwrap_or_default()
            .into_bytes(),
    )
}

/// Resolve the last snapshot per file path from a partition history, in
/// history order (last occurrence wins). Seed snapshots are excluded.
fn latest_snapshots_per_path(
    storage: &SqliteStorage,
    partition: &Partition,
) -> Result<Vec<(String, Snapshot)>, CheckpointError> {
    let mut order: Vec<String> = Vec::new();
    let mut last_per_path: HashMap<String, Snapshot> = HashMap::new();
    for snapshot_id in &partition.history {
        let snapshot = storage.get_snapshot(snapshot_id).map_err(map_layertwine_error)?;
        let path = snapshot_file_path(storage, &snapshot)?;
        if path == SEED_PATH {
            continue;
        }
        if !last_per_path.contains_key(&path) {
            order.push(path.clone());
        }
        last_per_path.insert(path, snapshot);
    }
    Ok(order
        .into_iter()
        .filter_map(|path| last_per_path.remove(&path).map(|snap| (path, snap)))
        .collect())
}

/// All partitions ordered by name (stable for tests).
pub fn list_partitions(storage: &SqliteStorage) -> Result<Vec<PartitionView>, CheckpointError> {
    let mut partitions = storage.list_partitions().map_err(map_layertwine_error)?;
    partitions.sort_by(|a, b| a.name.cmp(&b.name));
    let mut views = Vec::with_capacity(partitions.len());
    for partition in partitions {
        let (kind, actor) = match &partition.partition_type {
            PartitionType::Manual => ("manual", None),
            PartitionType::Agent(id) => ("agent", Some(id.0.clone())),
            PartitionType::Approval(id) => ("approval", Some(id.0.clone())),
            PartitionType::Integrated(name) => ("integrated", Some(name.clone())),
            PartitionType::Unified => ("unified", None),
            PartitionType::Staged => ("staged", None),
        };
        let mut created_at = 0;
        let mut updated_at = 0;
        for snapshot_id in &partition.history {
            if let Ok(snapshot) = storage.get_snapshot(snapshot_id) {
                if created_at == 0 {
                    created_at = snapshot.created_at;
                }
                updated_at = snapshot.created_at;
            }
        }
        views.push(PartitionView {
            partition_id: partition.id.to_string(),
            name: partition.name.clone(),
            kind: kind.to_string(),
            actor,
            current_snapshot: partition.current_snapshot.to_hex(),
            history_len: partition.history.len(),
            created_at,
            updated_at,
        });
    }
    Ok(views)
}

/// Changes recorded in an actor partition history, in chronological order
///.
///
/// `path_filter` is a plain substring match; `time_range` is
/// `[start, end]` milliseconds (inclusive), `None` = unbounded.
pub fn list_changes_by_actor(
    storage: &SqliteStorage,
    actor: &str,
    path_filter: Option<&str>,
    time_range: Option<(i64, i64)>,
) -> Result<Vec<DeltaSummary>, CheckpointError> {
    let partition = actor_partition(storage, actor)?;
    let mut changes = Vec::new();
    for snapshot_id in &partition.history {
        let snapshot = storage.get_snapshot(snapshot_id).map_err(map_layertwine_error)?;
        let path = snapshot_file_path(storage, &snapshot)?;
        if path == SEED_PATH || !path_matches(&path, path_filter) {
            continue;
        }
        if let Some((start, end)) = time_range {
            if snapshot.created_at < start || snapshot.created_at > end {
                continue;
            }
        }
        let source = snapshot_last_delta(storage, &snapshot)?
            .map(|d| source_label(&d.source))
            .unwrap_or_else(|| "agent".to_string());
        let content = snapshot_content_bytes(storage, &snapshot)?;
        changes.push(DeltaSummary {
            file: path,
            source,
            timestamp: snapshot.created_at,
            snapshot_id: snapshot.id.to_hex(),
            hash: sha256_hex(&content),
        });
    }
    Ok(changes)
}

/// Changes touching `path` across every partition (`list_changes_by_path`).
/// `time_range` (inclusive `(start, end)` timestamps) narrows the window.
pub fn list_changes_by_path(
    storage: &SqliteStorage,
    path: &str,
    time_range: Option<(i64, i64)>,
) -> Result<Vec<DeltaSummary>, CheckpointError> {
    let partitions = storage.list_partitions().map_err(map_layertwine_error)?;
    let mut changes = Vec::new();
    for partition in partitions {
        for snapshot_id in &partition.history {
            let snapshot = storage.get_snapshot(snapshot_id).map_err(map_layertwine_error)?;
            let snapshot_path = snapshot_file_path(storage, &snapshot)?;
            if snapshot_path == SEED_PATH || snapshot_path != path {
                continue;
            }
            if let Some((start, end)) = time_range {
                if snapshot.created_at < start || snapshot.created_at > end {
                    continue;
                }
            }
            let source = snapshot_last_delta(storage, &snapshot)?
                .map(|d| source_label(&d.source))
                .unwrap_or_else(|| "agent".to_string());
            let content = snapshot_content_bytes(storage, &snapshot)?;
            changes.push(DeltaSummary {
                file: snapshot_path,
                source,
                timestamp: snapshot.created_at,
                snapshot_id: snapshot.id.to_hex(),
                hash: sha256_hex(&content),
            });
        }
    }
    changes.sort_by_key(|c| c.timestamp);
    Ok(changes)
}

/// Reconstructed file set of an actor partition
/// `get_actor_workspace`).
pub fn get_actor_workspace(
    storage: &SqliteStorage,
    actor: &str,
) -> Result<Vec<WorkspaceFile>, CheckpointError> {
    let partition = actor_partition(storage, actor)?;
    let mut files = Vec::new();
    for (path, snapshot) in latest_snapshots_per_path(storage, &partition)? {
        // Deleted snapshots carry the explicit deletion marker: the path is
        // missing from the workspace rather than cleared.
        if snapshot.is_deleted() {
            continue;
        }
        let content = snapshot_content_bytes(storage, &snapshot)?;
        let hash = sha256_hex(&content);
        files.push(WorkspaceFile {
            path,
            content,
            hash,
            timestamp: snapshot.created_at,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// The staged partition's reconstructed file set (`diff_against_staged`
/// base). `workspace_key` selects the workspace-scoped staged partition
/// (`None` = legacy single-workspace fixed partition).
pub fn get_staged_workspace(
    storage: &SqliteStorage,
    workspace_key: Option<&str>,
) -> Result<Vec<WorkspaceFile>, CheckpointError> {
    let pid = staged_pid(workspace_key);
    let partition = storage.get_partition(&pid).map_err(map_layertwine_error)?;
    let mut files = Vec::new();
    for (path, snapshot) in latest_snapshots_per_path(storage, &partition)? {
        // Deleted snapshots carry the explicit deletion marker: the path is
        // missing from the staged workspace rather than cleared.
        if snapshot.is_deleted() {
            continue;
        }
        let content = snapshot_content_bytes(storage, &snapshot)?;
        let hash = sha256_hex(&content);
        files.push(WorkspaceFile {
            path,
            content,
            hash,
            timestamp: snapshot.created_at,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// A file whose merge snapshot carries the unresolved-conflict flag.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConflictFile {
    /// Relative file path.
    pub path: String,
    /// Snapshot id (hex) of the conflicted merge snapshot.
    pub snapshot_id: String,
    /// Partition the conflict lives in (`staged` / `integrated/<feature>`).
    pub partition: String,
    /// Re-derived conflict regions (best effort; empty when the merge
    /// inputs cannot be reconstructed from storage).
    pub conflicts: Vec<ConflictView>,
}

/// List files with unresolved merge conflicts across the staged and all
/// feature (integrated) partitions. Only the latest snapshot of each path
/// is considered (an older conflicted snapshot that was superseded by a
/// resolution no longer counts); the `MergeConflict` regions are re-derived
/// by replaying the merge over the snapshot's parents (best effort — old
/// snapshots whose inputs are gone report the path with an empty conflict
/// list).
pub fn list_conflicts(
    storage: &SqliteStorage,
    workspace_key: Option<&str>,
) -> Result<Vec<ConflictFile>, CheckpointError> {
    let mut partitions: Vec<Partition> = Vec::new();
    let staged_pid = staged_pid(workspace_key);
    if let Ok(partition) = storage.get_partition(&staged_pid) {
        partitions.push(partition);
    }
    for partition in storage.list_partitions().map_err(map_layertwine_error)? {
        if matches!(partition.partition_type, PartitionType::Integrated(_)) {
            partitions.push(partition);
        }
    }

    let mut out = Vec::new();
    for partition in partitions {
        for (path, snapshot) in latest_snapshots_per_path(storage, &partition)? {
            if !snapshot.has_conflicts {
                continue;
            }
            let conflicts = rederive_conflicts(storage, &snapshot)?;
            out.push(ConflictFile {
                path,
                snapshot_id: snapshot.id.to_hex(),
                partition: partition.name.clone(),
                conflicts,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Replay the three-way merge that produced a conflicted snapshot and
/// return the conflict regions as read views. The role of each parent is
/// derived from the snapshot's partition type and parent count, following
/// the `Snapshot::merge` conventions in layertwine. Returns an empty list
/// when the merge inputs cannot be reconstructed.
fn rederive_conflicts(
    storage: &SqliteStorage,
    snapshot: &Snapshot,
) -> Result<Vec<ConflictView>, CheckpointError> {
    let pt = &snapshot.partition_type;
    let parents = &snapshot.parents;

    // (base, ours, theirs) snapshot ids, when the merge shape is known.
    let roles: Option<(SnapshotId, SnapshotId, SnapshotId)> = if pt == "staged" {
        // merge_feature_to_staged: parents = [staged, feature]; base is the
        // feature partition's baseline (history[0]).
        if parents.len() >= 2 {
            let feature_snap = storage.get_snapshot(&parents[1]).map_err(map_layertwine_error)?;
            let name = feature_snap.partition_type.strip_prefix("integrated/");
            match name {
                Some(name) => {
                    let fpid = layertwine::layered::integrated::integrated_partition_id(name);
                    let base = storage
                        .get_partition(&fpid)
                        .ok()
                        .and_then(|p| p.history.first().copied());
                    base.map(|base| (base, parents[0], parents[1]))
                }
                None => None,
            }
        } else {
            None
        }
    } else if pt.starts_with("integrated/") {
        // merge_agent_to_feature: parents = [integrated, approval, baseline];
        // merge_texts(base=baseline, ours=approval, theirs=integrated).
        if parents.len() >= 3 {
            Some((parents[2], parents[1], parents[0]))
        } else {
            None
        }
    } else if pt.starts_with("approval/") {
        // move_agent_to_approval: parents = [approval, agent]; base is the
        // approval partition's baseline (history[0]).
        if parents.len() >= 2 {
            let agent = pt.strip_prefix("approval/");
            match agent {
                Some(agent) => {
                    let agent_id = AgentInstanceId(agent.to_string());
                    let pid = layertwine::layered::approval::approval_agent_partition_id(&agent_id);
                    let base = storage
                        .get_partition(&pid)
                        .ok()
                        .and_then(|p| p.history.first().copied());
                    base.map(|base| (base, parents[0], parents[1]))
                }
                None => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    let Some((base_id, ours_id, theirs_id)) = roles else {
        return Ok(vec![]);
    };
    let base = storage.get_snapshot(&base_id).map_err(map_layertwine_error)?;
    let ours = storage.get_snapshot(&ours_id).map_err(map_layertwine_error)?;
    let theirs = storage.get_snapshot(&theirs_id).map_err(map_layertwine_error)?;
    let base_text = layertwine::layered::transition::reconstruct_text(storage, &base)
        .map_err(map_layertwine_error)?
        .unwrap_or_default();
    let ours_text = layertwine::layered::transition::reconstruct_text(storage, &ours)
        .map_err(map_layertwine_error)?
        .unwrap_or_default();
    let theirs_text = layertwine::layered::transition::reconstruct_text(storage, &theirs)
        .map_err(map_layertwine_error)?
        .unwrap_or_default();
    let (_, conflicts) = merge_texts(&base_text, &ours_text, &theirs_text);
    let path = snapshot_file_path(storage, snapshot)?;
    Ok(to_conflict_views(&path, &conflicts))
}

/// Per-file diff between two workspace states
///. Binary files report `Modified` without a diff.
pub fn diff_workspaces(a: &[WorkspaceFile], b: &[WorkspaceFile]) -> Vec<FileDiffView> {
    let a_map: HashMap<&str, &WorkspaceFile> = a.iter().map(|f| (f.path.as_str(), f)).collect();
    let b_map: HashMap<&str, &WorkspaceFile> = b.iter().map(|f| (f.path.as_str(), f)).collect();

    let mut paths: Vec<&str> = a_map
        .keys()
        .chain(b_map.keys())
        .copied()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    paths.sort_unstable();

    let engine = DiffEngine::new().with_context_lines(3);
    let mut views = Vec::new();
    for path in paths {
        match (a_map.get(path), b_map.get(path)) {
            (None, Some(_bf)) => views.push(FileDiffView {
                path: path.to_string(),
                kind: FileDiffKind::Added,
                diff: None,
                additions: None,
                deletions: None,
            }),
            (Some(_af), None) => views.push(FileDiffView {
                path: path.to_string(),
                kind: FileDiffKind::Deleted,
                diff: None,
                additions: None,
                deletions: None,
            }),
            (None, None) => {} // unreachable: paths are the key union
            (Some(af), Some(bf)) => {
                if af.hash == bf.hash {
                    views.push(FileDiffView {
                        path: path.to_string(),
                        kind: FileDiffKind::Unchanged,
                        diff: None,
                        additions: None,
                        deletions: None,
                    });
                    continue;
                }
                let (diff, additions, deletions) = text_diff(&engine, &af.content, &bf.content);
                views.push(FileDiffView {
                    path: path.to_string(),
                    kind: FileDiffKind::Modified,
                    diff,
                    additions,
                    deletions,
                });
            }
        }
    }
    views
}

/// Build a unified diff when both contents are valid UTF-8 text, otherwise
/// `(None, None, None)` (binary).
fn text_diff(
    engine: &DiffEngine,
    before: &[u8],
    after: &[u8],
) -> (Option<String>, Option<usize>, Option<usize>) {
    let (Ok(before), Ok(after)) = (std::str::from_utf8(before), std::str::from_utf8(after)) else {
        return (None, None, None);
    };
    let diff = engine.unified_diff(before, after, None, None);
    let stats = engine.get_stats(before, after);
    (
        Some(diff),
        Some(stats.added_lines),
        Some(stats.removed_lines),
    )
}

/// Diff between two actor workspaces.
pub fn diff_actors(
    storage: &SqliteStorage,
    actor_a: &str,
    actor_b: &str,
) -> Result<Vec<FileDiffView>, CheckpointError> {
    let a = get_actor_workspace(storage, actor_a)?;
    let b = get_actor_workspace(storage, actor_b)?;
    Ok(diff_workspaces(&a, &b))
}

/// Diff between an actor workspace and the staged partition
///.
pub fn diff_against_staged(
    storage: &SqliteStorage,
    actor: &str,
    workspace_key: Option<&str>,
) -> Result<Vec<FileDiffView>, CheckpointError> {
    let actor_files = get_actor_workspace(storage, actor)?;
    let staged_files = get_staged_workspace(storage, workspace_key)?;
    Ok(diff_workspaces(&actor_files, &staged_files))
}

/// Convert a workspace file set into content entries (used by restore
/// callers / API projections).
pub fn workspace_entries(files: &[WorkspaceFile]) -> Vec<FileContentEntry> {
    files
        .iter()
        .map(|f| FileContentEntry::new(f.path.clone(), f.content.clone()))
        .collect()
}

/// Resolve the actor partition of an actor id string (full `ActorId` or bare
/// execution id, mirroring `FileCheckpointManager::actor_id_for`).
fn actor_partition(storage: &SqliteStorage, actor: &str) -> Result<Partition, CheckpointError> {
    let actor = match ActorId::parse(actor) {
        Ok(parsed) => parsed,
        Err(_) => ActorId::new(
            crate::actor_id::ActorKind::Agent,
            &[wf_types::Id::from(actor.to_string())],
        )
        .map_err(|e| CheckpointError::Validation {
            reason: format!("invalid actor id '{actor}': {e}"),
        })?,
    };
    let agent_id = actor.to_agent_instance_id();
    let pid = layertwine::layered::agent::agent_partition_id(&agent_id);
    storage
        .get_partition(&pid)
        .map_err(|_| CheckpointError::NotFound {
            id: format!("actor partition for '{actor}'"),
        })
}

/// Actor id of a delta source (provenance display).
fn source_label(source: &SourceType) -> String {
    match source {
        SourceType::Manual => "manual".to_string(),
        SourceType::Agent(id) => id.0.clone(),
        SourceType::Backup => "backup".to_string(),
    }
}

impl DeltaSummary {
    /// Map a snapshot chain to its change summary.
    pub fn from_snapshot(
        storage: &SqliteStorage,
        snapshot: &Snapshot,
    ) -> Result<Option<DeltaSummary>, CheckpointError> {
        let path = snapshot_file_path(storage, snapshot)?;
        if path == SEED_PATH {
            return Ok(None);
        }
        let source = snapshot_last_delta(storage, snapshot)?
            .map(|d| source_label(&d.source))
            .unwrap_or_else(|| "agent".to_string());
        let content = snapshot_content_bytes(storage, snapshot)?;
        Ok(Some(DeltaSummary {
            file: path,
            source,
            timestamp: snapshot.created_at,
            snapshot_id: snapshot.id.to_hex(),
            hash: sha256_hex(&content),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor_id::{ActorId, ActorKind};
    use crate::file::FileCheckpointManager;
    use wf_types::Id;

    fn actor(kind: ActorKind, id: &str) -> ActorId {
        ActorId::new(kind, &[Id::from(id.to_string())]).unwrap()
    }

    #[test]
    fn list_changes_by_actor_reports_edits_in_order() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let a = actor(ActorKind::Agent, "loop-1");
        manager.apply_agent_edit(&a, "src/a.txt", b"one\n").unwrap();
        manager
            .apply_agent_edit(&a, "src/a.txt", b"one\ntwo\n")
            .unwrap();
        manager
            .apply_agent_edit(&a, "bin.dat", b"\x00\x01\x02")
            .unwrap();

        let storage = manager.storage().unwrap();
        let changes = list_changes_by_actor(storage, a.as_str(), None, None).unwrap();
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].file, "src/a.txt");
        assert_eq!(changes[0].source, a.as_str());
        assert!(changes[1].timestamp >= changes[0].timestamp);
        assert_eq!(changes[2].file, "bin.dat");
    }

    #[test]
    fn list_changes_filters_by_path_and_time() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let a = actor(ActorKind::Agent, "loop-1");
        manager.apply_agent_edit(&a, "src/a.txt", b"x\n").unwrap();
        manager
            .apply_agent_edit(&a, "docs/readme.md", b"y\n")
            .unwrap();

        let storage = manager.storage().unwrap();
        let filtered = list_changes_by_actor(storage, a.as_str(), Some("docs"), None).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].file, "docs/readme.md");

        let changes = list_changes_by_actor(storage, a.as_str(), None, None).unwrap();
        let first_ts = changes[0].timestamp;
        let empty = list_changes_by_actor(
            storage,
            a.as_str(),
            None,
            Some((first_ts - 1, first_ts - 1)),
        )
        .unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn list_changes_by_path_scans_all_partitions() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let a = actor(ActorKind::Agent, "loop-1");
        let b = actor(ActorKind::Agent, "loop-2");
        manager.apply_agent_edit(&a, "shared.txt", b"a\n").unwrap();
        manager.apply_agent_edit(&b, "shared.txt", b"b\n").unwrap();
        manager.apply_agent_edit(&a, "other.txt", b"c\n").unwrap();

        let storage = manager.storage().unwrap();
        let changes = list_changes_by_path(storage, "shared.txt", None).unwrap();
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| c.file == "shared.txt"));
    }

    #[test]
    fn get_actor_workspace_reconstructs_latest_state() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let a = actor(ActorKind::Agent, "loop-1");
        manager.apply_agent_edit(&a, "a.txt", b"v1\n").unwrap();
        manager.apply_agent_edit(&a, "a.txt", b"v1\nv2\n").unwrap();
        manager.apply_agent_edit(&a, "b.txt", b"data").unwrap();

        let storage = manager.storage().unwrap();
        let workspace = get_actor_workspace(storage, a.as_str()).unwrap();
        assert_eq!(workspace.len(), 2);
        let a_txt = workspace.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!(a_txt.content, b"v1\nv2\n");
    }

    #[test]
    fn diff_workspaces_reports_add_remove_modify() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let a = actor(ActorKind::Agent, "loop-1");
        let b = actor(ActorKind::Agent, "loop-2");
        manager.apply_agent_edit(&a, "same.txt", b"x\n").unwrap();
        manager.apply_agent_edit(&a, "only-a.txt", b"a\n").unwrap();
        manager
            .apply_agent_edit(&a, "changed.txt", b"one\n")
            .unwrap();
        manager.apply_agent_edit(&b, "same.txt", b"x\n").unwrap();
        manager.apply_agent_edit(&b, "only-b.txt", b"b\n").unwrap();
        manager
            .apply_agent_edit(&b, "changed.txt", b"one\ntwo\n")
            .unwrap();

        let storage = manager.storage().unwrap();
        let diffs = diff_actors(storage, a.as_str(), b.as_str()).unwrap();
        let kinds: HashMap<&str, FileDiffKind> =
            diffs.iter().map(|d| (d.path.as_str(), d.kind)).collect();
        assert_eq!(kinds.get("same.txt"), Some(&FileDiffKind::Unchanged));
        assert_eq!(kinds.get("only-a.txt"), Some(&FileDiffKind::Deleted));
        assert_eq!(kinds.get("only-b.txt"), Some(&FileDiffKind::Added));
        assert_eq!(kinds.get("changed.txt"), Some(&FileDiffKind::Modified));
        let changed = diffs.iter().find(|d| d.path == "changed.txt").unwrap();
        assert!(changed.diff.as_ref().unwrap().contains("+two"));
    }

    #[test]
    fn list_partitions_views_actor_and_kind() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let a = actor(ActorKind::Agent, "loop-1");
        manager.apply_agent_edit(&a, "a.txt", b"x\n").unwrap();

        let storage = manager.storage().unwrap();
        let views = list_partitions(storage).unwrap();
        let agent_view = views.iter().find(|v| v.kind == "agent").unwrap();
        assert_eq!(agent_view.actor.as_deref(), Some(a.as_str()));
        assert!(agent_view.history_len >= 2);
        assert!(agent_view.created_at > 0);
        assert!(agent_view.updated_at >= agent_view.created_at);
    }

    #[test]
    fn nested_actor_partitions_are_isolated() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let parent = actor(ActorKind::Wf, "wf-1");
        let child1 = parent.child(&Id::from("sub-1".to_string())).unwrap();
        let child2 = parent.child(&Id::from("sub-2".to_string())).unwrap();

        manager
            .apply_agent_edit(&child1, "a.txt", b"child1\n")
            .unwrap();
        manager
            .apply_agent_edit(&child2, "a.txt", b"child2\n")
            .unwrap();

        let storage = manager.storage().unwrap();
        let ws1 = get_actor_workspace(storage, child1.as_str()).unwrap();
        let ws2 = get_actor_workspace(storage, child2.as_str()).unwrap();
        assert_eq!(ws1[0].content, b"child1\n");
        assert_eq!(ws2[0].content, b"child2\n");
        assert_ne!(ws1[0].hash, ws2[0].hash);
    }

    #[test]
    fn workspace_entries_convert_to_content_entries() {
        let files = vec![WorkspaceFile {
            path: "a.txt".to_string(),
            content: b"x".to_vec(),
            hash: "h".to_string(),
            timestamp: 1,
        }];
        let entries = workspace_entries(&files);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "a.txt");
        assert_eq!(entries[0].content, b"x");
    }

    #[test]
    fn missing_actor_partition_reports_not_found() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let storage = manager.storage().unwrap();
        let err = get_actor_workspace(storage, "agent:ghost").unwrap_err();
        assert!(matches!(err, CheckpointError::NotFound { .. }));
    }

    #[test]
    fn binary_content_is_snapshotted_verbatim() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let a = actor(ActorKind::Agent, "loop-bin");
        manager
            .apply_agent_edit(&a, "img.bin", b"\x00\xFF\x10")
            .unwrap();

        let storage = manager.storage().unwrap();
        let ws = get_actor_workspace(storage, a.as_str()).unwrap();
        assert_eq!(ws[0].content, b"\x00\xFF\x10");
    }

    #[test]
    fn no_map_storage_remnants() {
        // Layertwine failures flow exclusively through
        // `crate::file_util::map_layertwine_error`; the historical duplicate
        // helpers must stay deleted. Needles are assembled from fragments so
        // this test's own source cannot match them.
        let provenance_needle = ["fn map_", "storage"].concat();
        let adapter_needle = ["fn map_", "storage_", "result"].concat();
        let provenance_src = include_str!("provenance.rs");
        assert!(
            !provenance_src.contains(&provenance_needle),
            "duplicate error mapper must not be reintroduced in provenance.rs"
        );
        let adapter_src = include_str!("layertwine.rs");
        assert!(
            !adapter_src.contains(&adapter_needle),
            "duplicate error mapper must not be reintroduced in layertwine.rs"
        );
    }
}
