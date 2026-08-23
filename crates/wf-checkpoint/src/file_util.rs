use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use layertwine::checkpoint::types::Checkpoint;
use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::partition::Partition;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::{AgentInstanceId, LineDiff, SnapshotId, SourceType};
use layertwine::storage::repository::{DeltaStore, FileNodeStore, SnapshotStore};
use layertwine::storage::sqlite::SqliteStorage;
use sha2::{Digest, Sha256};
use wf_types::config::file_checkpoint::FailureBehavior;

use crate::error::CheckpointError;
use crate::file::{FileCheckpoint, FileState};

/// SHA-256 hex digest of a byte slice.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// Path of the synthetic initial snapshot seeding an actor partition.
pub(crate) const SEED_PATH: &str = ".wf-checkpoint-seed";

/// Normalize a workspace root into the stable workspace key used to derive
/// workspace-scoped manual/staged partition ids: trailing path separators
/// are stripped, everything else is kept verbatim so the same root always
/// maps to the same key.
pub(crate) fn normalize_workspace_key(root: &Path) -> String {
    let raw = root.to_string_lossy();
    let trimmed = raw.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        raw.into_owned()
    } else {
        trimmed.to_string()
    }
}

/// Map a layertwine error into the unified `CheckpointError`.
pub(crate) fn map_layertwine_error<E: Into<layertwine::LayertwineError>>(e: E) -> CheckpointError {
    match e.into() {
        layertwine::LayertwineError::NotFound(id) => CheckpointError::NotFound { id },
        layertwine::LayertwineError::Storage(err) => match err {
            layertwine::StorageError::NotFound(id) => CheckpointError::NotFound { id },
            other => CheckpointError::Internal(format!("layertwine: {other}")),
        },
        other => CheckpointError::Internal(format!("layertwine: {other}")),
    }
}

/// The file path a snapshot applies to.
pub(crate) fn snapshot_file_path(
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

/// Reconstruct the byte content of a snapshot.
pub(crate) fn snapshot_content_bytes(
    storage: &SqliteStorage,
    snapshot: &Snapshot,
) -> Result<Vec<u8>, CheckpointError> {
    if let Some(content) = &snapshot.content {
        return Ok(content.to_bytes());
    }
    Ok(
        layertwine::layered::transition::reconstruct_text(storage, snapshot)
            .map_err(map_layertwine_error)?
            .unwrap_or_default()
            .into_bytes(),
    )
}

pub(crate) fn checkpoint_states(
    storage: &SqliteStorage,
    checkpoint: &Checkpoint,
) -> Result<Vec<(String, Vec<u8>, i64)>, CheckpointError> {
    let mut states = Vec::with_capacity(checkpoint.baseline_snapshots.len());
    for snapshot_id in &checkpoint.baseline_snapshots {
        let snapshot = storage
            .get_snapshot(snapshot_id)
            .map_err(map_layertwine_error)?;
        let path = snapshot_file_path(storage, &snapshot)?;
        if path == SEED_PATH {
            continue;
        }
        let bytes = snapshot_content_bytes(storage, &snapshot)?;
        states.push((path, bytes, snapshot.created_at));
    }
    Ok(states)
}

pub(crate) fn checkpoint_deleted_paths(
    storage: &SqliteStorage,
    checkpoint: &Checkpoint,
) -> Result<HashSet<String>, CheckpointError> {
    let mut deleted = HashSet::new();
    for snapshot_id in &checkpoint.baseline_snapshots {
        let snapshot = storage
            .get_snapshot(snapshot_id)
            .map_err(map_layertwine_error)?;
        if snapshot.is_deleted() {
            let path = snapshot_file_path(storage, &snapshot)?;
            deleted.insert(path);
        }
    }
    Ok(deleted)
}

/// Seed a fresh initial snapshot for a partition.
pub(crate) fn seed_initial_snapshot(
    storage: &SqliteStorage,
    agent_id: &AgentInstanceId,
) -> Result<SnapshotId, CheckpointError> {
    let file_node = FileNode::new(PathBuf::from(SEED_PATH), b"");
    storage
        .store_file_node(&file_node, b"")
        .map_err(map_layertwine_error)?;
    let delta = Delta::new(
        file_node.clone(),
        LineDiff::new(vec![]),
        SourceType::Agent(agent_id.clone()),
    );
    storage.store_delta(&delta).map_err(map_layertwine_error)?;
    let snapshot = Snapshot::new_initial(file_node, delta.id);
    storage
        .store_snapshot(&snapshot, b"")
        .map_err(map_layertwine_error)?;
    Ok(snapshot.id)
}

/// Latest snapshot id per file path in the partition history.
pub(crate) fn partition_latest_snapshot_ids(
    storage: &SqliteStorage,
    partition: &Partition,
) -> Result<Vec<SnapshotId>, CheckpointError> {
    let mut last_per_path: HashMap<String, SnapshotId> = HashMap::new();
    for snapshot_id in &partition.history {
        let snapshot = storage
            .get_snapshot(snapshot_id)
            .map_err(map_layertwine_error)?;
        let path = snapshot_file_path(storage, &snapshot)?;
        last_per_path.insert(path, *snapshot_id);
    }
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    for snapshot_id in &partition.history {
        let snapshot = storage
            .get_snapshot(snapshot_id)
            .map_err(map_layertwine_error)?;
        let path = snapshot_file_path(storage, &snapshot)?;
        if last_per_path.get(&path) == Some(snapshot_id) && path != SEED_PATH && seen.insert(path) {
            ids.push(*snapshot_id);
        }
    }
    Ok(ids)
}

/// SHA-256 of the sorted `path=hash;` pairs (stable workspace fingerprint).
pub(crate) fn compute_full_hash(files: &[FileState]) -> String {
    let mut parts: Vec<&FileState> = files.iter().collect();
    parts.sort_by(|a, b| a.path.cmp(&b.path));
    let mut hasher = Sha256::new();
    for f in parts {
        hasher.update(f.path.as_bytes());
        hasher.update(b"=");
        hasher.update(f.hash.as_bytes());
        hasher.update(b";");
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

pub(crate) fn write_file_with_dirs(target: &Path, content: &[u8]) -> Result<(), std::io::Error> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, content)
}

pub(crate) fn handle_restore_failure(
    behavior: FailureBehavior,
    path: &str,
    err: &std::io::Error,
) -> Result<(), CheckpointError> {
    match behavior {
        FailureBehavior::Error => Err(CheckpointError::Io(std::io::Error::other(format!(
            "failed to restore '{path}': {err}"
        )))),
        FailureBehavior::Warn => {
            tracing::warn!("failed to restore '{path}': {err}");
            Ok(())
        }
        FailureBehavior::Ignore => Ok(()),
    }
}

/// Resolve the on-disk target for a restored file state.
pub(crate) fn resolve_restore_target(
    base_dir: &Path,
    path: &str,
) -> Result<PathBuf, CheckpointError> {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return Ok(candidate);
    }
    let joined = base_dir.join(&candidate);
    let normalized = joined
        .canonicalize()
        .unwrap_or_else(|_| normalize_lexically(&joined));
    let base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| normalize_lexically(base_dir));
    if !normalized.starts_with(&base) {
        return Err(CheckpointError::Validation {
            reason: format!(
                "file checkpoint path '{}' escapes base directory '{}'",
                path,
                base_dir.display()
            ),
        });
    }
    Ok(joined)
}

/// Lexical normalization without touching the filesystem.
pub(crate) fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Fallback root actor for a bare execution id (agent kind).
pub(crate) fn root_actor(execution_id: wf_types::Id) -> crate::actor_id::ActorId {
    crate::actor_id::ActorId::new(crate::actor_id::ActorKind::Agent, &[execution_id])
        .unwrap_or_else(|_| {
            crate::actor_id::ActorId::new(
                crate::actor_id::ActorKind::Agent,
                &[wf_types::Id::from("unknown")],
            )
            .unwrap()
        })
}

/// Build the projection of a layertwine checkpoint.
pub(crate) fn projection(
    storage: &SqliteStorage,
    checkpoint: &Checkpoint,
    deleted: &HashSet<String>,
) -> Result<FileCheckpoint, CheckpointError> {
    let states = checkpoint_states(storage, checkpoint)?;
    let mut files = Vec::with_capacity(states.len());
    for (path, content, ts) in states {
        files.push(FileState {
            deleted: deleted.contains(&path),
            path,
            hash: sha256_hex(&content),
            size: content.len() as u64,
            last_modified: ts,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let full_hash = compute_full_hash(&files);
    Ok(FileCheckpoint {
        id: checkpoint.id.to_hex(),
        timestamp: checkpoint.created_at,
        full_hash,
        files,
        checkpoint_type: "full".to_string(),
        base_checkpoint_id: None,
        changes: None,
        empty_dirs: None,
    })
}
