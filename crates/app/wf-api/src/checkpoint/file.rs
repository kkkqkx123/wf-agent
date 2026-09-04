//! File checkpoint API: workspace scanning, diff queries, file change
//! monitoring, and workspace-level checkpoint operations backed by the
//! `FileCheckpointManager`.

use std::path::Path;

use serde::Serialize;

use wf_checkpoint::file::{FileCheckpointManager, FileCheckpointOptions, FileState};
use wf_checkpoint::provenance::{DeltaSummary, FileDiffView, PartitionView, WorkspaceFile};
use wf_checkpoint::scan::{ScanConfig, WorkspaceScanner};
use wf_checkpoint::watcher::{FileChangeKind, FileChangeRecord};

use crate::ApiResult;

/// Summary view of a file checkpoint for API responses.
#[derive(Debug, Clone, Serialize)]
pub struct FileCheckpointSummary {
    pub id: String,
    pub timestamp: i64,
    pub file_count: usize,
    pub checkpoint_type: String,
}

/// Result of scanning a workspace directory.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceScanResult {
    pub files: Vec<FileState>,
    pub dirs: Vec<String>,
    pub empty_dirs: Vec<String>,
}

/// Create a full file checkpoint for the given workspace root.
pub fn create_file_checkpoint(
    manager: &FileCheckpointManager,
    actor_id: &str,
    workspace_root: &Path,
) -> ApiResult<FileCheckpointSummary> {
    let opts = FileCheckpointOptions::default();
    let checkpoint = manager
        .create_workspace_checkpoint(actor_id, workspace_root, &opts)
        .map_err(crate::ApiError::execution_with_source)?;
    Ok(FileCheckpointSummary {
        id: checkpoint.id,
        timestamp: checkpoint.timestamp,
        file_count: checkpoint.files.len(),
        checkpoint_type: checkpoint.checkpoint_type,
    })
}

/// Restore workspace files from a specific checkpoint.
pub fn restore_workspace_from_checkpoint(
    manager: &FileCheckpointManager,
    entity_id: &str,
    checkpoint_id: &str,
    workspace_root: &Path,
) -> ApiResult<usize> {
    let opts = FileCheckpointOptions::default();
    let result = manager
        .restore_workspace(entity_id, checkpoint_id, workspace_root, &opts)
        .map_err(crate::ApiError::execution_with_source)?;
    Ok(result.restored)
}

/// Scan a workspace directory and return the list of tracked files.
pub fn scan_workspace(
    workspace_root: &Path,
    ignore_patterns: &[String],
) -> ApiResult<WorkspaceScanResult> {
    let config = ScanConfig {
        custom_ignore_patterns: ignore_patterns.to_vec(),
        ..Default::default()
    };
    let scanner = WorkspaceScanner::new(config);
    let scan = scanner
        .scan(workspace_root)
        .map_err(crate::ApiError::execution_with_source)?;
    Ok(WorkspaceScanResult {
        files: scan.files,
        dirs: scan.dirs,
        empty_dirs: scan.empty_dirs,
    })
}

/// List file changes recorded by an actor partition.
pub fn list_file_changes(
    manager: &FileCheckpointManager,
    actor: &str,
) -> ApiResult<Vec<DeltaSummary>> {
    let changes = manager
        .list_changes_by_actor(actor, None, None)
        .map_err(crate::ApiError::execution_with_source)?;
    Ok(changes)
}

/// Get the per-file diff view between two actor workspaces.
pub fn diff_actors(
    manager: &FileCheckpointManager,
    actor_a: &str,
    actor_b: &str,
) -> ApiResult<Vec<FileDiffView>> {
    let diff = manager
        .diff_actors(actor_a, actor_b)
        .map_err(crate::ApiError::execution_with_source)?;
    Ok(diff)
}

/// Get the diff between an actor workspace and the staged partition.
pub fn diff_against_staged(
    manager: &FileCheckpointManager,
    actor: &str,
) -> ApiResult<Vec<FileDiffView>> {
    let diff = manager
        .diff_against_staged(actor)
        .map_err(crate::ApiError::execution_with_source)?;
    Ok(diff)
}

/// List all partitions for the checkpoint store.
pub fn list_partitions(manager: &FileCheckpointManager) -> ApiResult<Vec<PartitionView>> {
    let partitions = manager
        .list_partitions()
        .map_err(crate::ApiError::execution_with_source)?;
    Ok(partitions)
}

/// Get the reconstructed file set of an actor partition.
pub fn get_actor_workspace(
    manager: &FileCheckpointManager,
    actor: &str,
) -> ApiResult<Vec<WorkspaceFile>> {
    let files = manager
        .get_actor_workspace(actor)
        .map_err(crate::ApiError::execution_with_source)?;
    Ok(files)
}

/// List files with unresolved merge conflicts.
pub fn list_conflicts(
    manager: &FileCheckpointManager,
) -> ApiResult<Vec<wf_checkpoint::provenance::ConflictFile>> {
    let conflicts = manager
        .list_conflicts()
        .map_err(crate::ApiError::execution_with_source)?;
    Ok(conflicts)
}

/// Filter file change records by kind.
pub fn filter_changes_by_kind(
    changes: &[FileChangeRecord],
    kind: FileChangeKind,
) -> Vec<&FileChangeRecord> {
    changes.iter().filter(|c| c.kind == kind).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_changes_by_kind_works() {
        let changes = vec![
            FileChangeRecord {
                path: std::path::PathBuf::from("a.txt"),
                kind: FileChangeKind::Change,
                timestamp: 1000,
            },
            FileChangeRecord {
                path: std::path::PathBuf::from("b.txt"),
                kind: FileChangeKind::Add,
                timestamp: 2000,
            },
            FileChangeRecord {
                path: std::path::PathBuf::from("c.txt"),
                kind: FileChangeKind::Change,
                timestamp: 3000,
            },
        ];
        let modified = filter_changes_by_kind(&changes, FileChangeKind::Change);
        assert_eq!(modified.len(), 2);
        let created = filter_changes_by_kind(&changes, FileChangeKind::Add);
        assert_eq!(created.len(), 1);
    }
}
