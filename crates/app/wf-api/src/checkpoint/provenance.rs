use wf_checkpoint::provenance::{DeltaSummary, FileDiffView, PartitionView, WorkspaceFile};
use wf_checkpoint::{EditSession, GcRetention, GcStats};

use crate::infra::context::ApiContext;
use crate::ApiError;
use crate::ApiResult;

/// The attached file checkpoint manager, or an error when file
/// checkpointing is disabled.
fn manager(ctx: &ApiContext) -> ApiResult<&wf_checkpoint::file::FileCheckpointManager> {
    ctx.file_checkpoint_manager().ok_or_else(|| {
        ApiError::execution("file checkpointing is not enabled; set file_checkpoint.enabled=true")
    })
}

/// All partitions of the file-checkpoint store (actor partitions, approval,
/// integrated features, staged).
pub fn list_partitions(ctx: &ApiContext) -> ApiResult<Vec<PartitionView>> {
    manager(ctx)?
        .list_partitions()
        .map_err(ApiError::execution_with_source)
}

/// Changes recorded in an actor partition, in chronological order, with
/// optional path substring filter and inclusive time window.
pub fn list_changes_by_actor(
    ctx: &ApiContext,
    actor: &str,
    path_filter: Option<&str>,
    time_range: Option<(i64, i64)>,
) -> ApiResult<Vec<DeltaSummary>> {
    manager(ctx)?
        .list_changes_by_actor(actor, path_filter, time_range)
        .map_err(ApiError::execution_with_source)
}

/// Changes touching a path across every partition, with an optional
/// inclusive time window.
pub fn list_changes_by_path(
    ctx: &ApiContext,
    path: &str,
    time_range: Option<(i64, i64)>,
) -> ApiResult<Vec<DeltaSummary>> {
    manager(ctx)?
        .list_changes_by_path(path, time_range)
        .map_err(ApiError::execution_with_source)
}

/// Reconstructed file set of an actor partition (current state).
pub fn get_actor_workspace(ctx: &ApiContext, actor: &str) -> ApiResult<Vec<WorkspaceFile>> {
    manager(ctx)?
        .get_actor_workspace(actor)
        .map_err(ApiError::execution_with_source)
}

/// Per-file diff between two actor workspaces.
pub fn diff_actors(ctx: &ApiContext, actor_a: &str, actor_b: &str) -> ApiResult<Vec<FileDiffView>> {
    manager(ctx)?
        .diff_actors(actor_a, actor_b)
        .map_err(ApiError::execution_with_source)
}

/// Per-file diff between an actor workspace and the staged partition.
pub fn diff_against_staged(ctx: &ApiContext, actor: &str) -> ApiResult<Vec<FileDiffView>> {
    manager(ctx)?
        .diff_against_staged(actor)
        .map_err(ApiError::execution_with_source)
}

/// Complete version timeline for a file path, including rename/move history.
pub fn file_timeline(
    ctx: &ApiContext,
    path: &str,
) -> ApiResult<wf_checkpoint::provenance::FileTimeline> {
    let storage = manager(ctx)?
        .storage()
        .ok_or_else(|| ApiError::execution("file checkpoint storage is not configured"))?;
    wf_checkpoint::provenance::file_timeline(storage, path).map_err(ApiError::execution_with_source)
}

/// Maximum entries returned by a file timeline view.
pub const MAX_FILE_TIMELINE_ENTRIES: usize = 5000;

/// Capped file timeline view with total before truncation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileTimelineView {
    pub original_path: String,
    pub entries: Vec<wf_checkpoint::provenance::FileTimelineEntry>,
    pub truncated: bool,
    pub total: usize,
}

/// File timeline with cap and truncation flag. Entries are chronological, so
/// the earliest page is stable.
pub fn file_timeline_capped(ctx: &ApiContext, path: &str) -> ApiResult<FileTimelineView> {
    let timeline = file_timeline(ctx, path)?;
    let total = timeline.entries.len();
    let truncated = total > MAX_FILE_TIMELINE_ENTRIES;
    let mut entries = timeline.entries;
    entries.truncate(MAX_FILE_TIMELINE_ENTRIES);
    Ok(FileTimelineView {
        original_path: timeline.original_path,
        entries,
        truncated,
        total,
    })
}

/// Explicit rename: record the move linkage and apply it as delete-old +
/// edit-new for the actor's partition. Returns the new snapshot id (hex).
pub fn rename_file(
    ctx: &ApiContext,
    actor: &str,
    from_path: &str,
    to_path: &str,
    content: &[u8],
) -> ApiResult<String> {
    let manager = manager(ctx)?;
    let actor_id = manager.actor_id_for(actor);
    manager
        .rename_file(&actor_id, from_path, to_path, content)
        .map_err(ApiError::execution_with_source)
}

/// Begin a new edit group for grouping a multi-file operation.
pub fn begin_edit_group(ctx: &ApiContext, label: Option<String>) -> ApiResult<String> {
    manager(ctx)?
        .begin_edit_group(label)
        .map(|id| id.to_string())
        .map_err(ApiError::execution_with_source)
}

/// List all persisted edit sessions (newest first).
pub fn list_sessions(ctx: &ApiContext) -> ApiResult<Vec<EditSession>> {
    manager(ctx)?
        .list_sessions()
        .map_err(ApiError::execution_with_source)
}

/// Roll back an entire edit session on an actor's partition.
pub fn rollback_session(ctx: &ApiContext, actor: &str, session_id: &str) -> ApiResult<String> {
    let session_id = session_id
        .parse()
        .map_err(|_| ApiError::execution(format!("invalid session id '{session_id}'")))?;
    manager(ctx)?
        .rollback_session(actor, &session_id)
        .map_err(ApiError::execution_with_source)
}

/// Undo the last edit on an actor's partition.
pub fn undo_edit(ctx: &ApiContext, actor: &str) -> ApiResult<String> {
    manager(ctx)?
        .undo_edit(actor)
        .map_err(ApiError::execution_with_source)
}

/// Redo the most recently undone edit on an actor's partition.
pub fn redo_edit(ctx: &ApiContext, actor: &str) -> ApiResult<String> {
    manager(ctx)?
        .redo_edit(actor)
        .map_err(ApiError::execution_with_source)
}

/// Trigger a manual GC run on the file-checkpoint store. `keep_recent_heads`
/// controls how many recent partition heads are kept protected beyond the
/// built-in protected set (branch heads + ancestors + git anchors).
pub fn run_gc(ctx: &ApiContext, keep_recent_heads: usize) -> ApiResult<GcStats> {
    let retention = GcRetention { keep_recent_heads };
    manager(ctx)?
        .run_gc(retention)
        .map_err(ApiError::execution_with_source)
}

/// Maximum bytes returned by a single file content read; larger files are
/// truncated with `truncated: true`.
pub const MAX_FILE_CONTENT_BYTES: usize = 1024 * 1024;
/// Maximum entries returned by a directory tree listing.
pub const MAX_TREE_ENTRIES: usize = 2000;

/// Read-only view of one workspace file. `version` is `None` for workspace
/// current state; snapshot-versioned reads are distinguished by callers via
/// the file timeline (snapshot ids + hashes) and remain timeline-anchored.
/// Direct workspace mutations (rename, sessions, undo/redo) are explicit file
/// operations; the approval channel covers human-in-the-loop tool approvals.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileContentView {
    pub path: String,
    pub actor: String,
    pub hash: String,
    pub size: usize,
    pub is_binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default)]
    pub truncated: bool,
    pub timestamp: i64,
}

/// One directory tree entry (metadata only, no content bytes).
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileTreeEntry {
    pub path: String,
    pub hash: String,
    pub size: usize,
    pub timestamp: i64,
}

/// Validate a workspace-relative path against sandbox escape.
pub fn validate_workspace_path(path: &str) -> ApiResult<()> {
    if path.is_empty() || path.len() > 1024 {
        return Err(ApiError::Validation(
            "path must be 1..=1024 chars".to_string(),
        ));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(ApiError::Validation(
            "absolute paths are rejected".to_string(),
        ));
    }
    for component in path.split('/') {
        if component == ".." {
            return Err(ApiError::Validation(
                "path escapes the workspace".to_string(),
            ));
        }
    }
    if path.contains('\\') || path.contains('\0') {
        return Err(ApiError::Validation(
            "path contains illegal characters".to_string(),
        ));
    }
    Ok(())
}

fn is_binary_bytes(content: &[u8]) -> bool {
    content.iter().take(8000).any(|b| *b == 0)
}

/// Read the current workspace content of one file with sandbox checks.
pub fn read_file_content(ctx: &ApiContext, actor: &str, path: &str) -> ApiResult<FileContentView> {
    validate_workspace_path(path)?;
    let files = get_actor_workspace(ctx, actor)?;
    let Some(file) = files.into_iter().find(|f| f.path == path) else {
        return Err(ApiError::not_found("file", path));
    };
    let binary = is_binary_bytes(&file.content);
    let truncated = file.content.len() > MAX_FILE_CONTENT_BYTES;
    let capped = file
        .content
        .get(..MAX_FILE_CONTENT_BYTES.min(file.content.len()))
        .unwrap_or_default();
    let content = if binary {
        None
    } else {
        Some(String::from_utf8_lossy(capped).into_owned())
    };
    Ok(FileContentView {
        path: file.path,
        actor: actor.to_string(),
        hash: file.hash,
        size: file.content.len(),
        is_binary: binary,
        content,
        truncated,
        timestamp: file.timestamp,
    })
}

/// Capped directory tree view with total before truncation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileTreeView {
    pub entries: Vec<FileTreeEntry>,
    pub truncated: bool,
    pub total: usize,
}

/// List workspace tree entries with an optional path prefix filter.
pub fn list_tree(
    ctx: &ApiContext,
    actor: &str,
    prefix: Option<&str>,
) -> ApiResult<Vec<FileTreeEntry>> {
    Ok(list_tree_capped(ctx, actor, prefix)?.entries)
}

/// List workspace tree entries with cap and truncation flag. Entries are
/// sorted by path before truncation so the first page is stable.
pub fn list_tree_capped(
    ctx: &ApiContext,
    actor: &str,
    prefix: Option<&str>,
) -> ApiResult<FileTreeView> {
    if let Some(prefix) = prefix {
        if !prefix.is_empty() {
            validate_workspace_path(prefix)?;
        }
    }
    let files = get_actor_workspace(ctx, actor)?;
    let mut out = Vec::new();
    for file in files {
        if let Some(prefix) = prefix {
            if !prefix.is_empty() && !file.path.starts_with(prefix) {
                continue;
            }
        }
        out.push(FileTreeEntry {
            path: file.path,
            hash: file.hash,
            size: file.content.len(),
            timestamp: file.timestamp,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    let total = out.len();
    let truncated = total > MAX_TREE_ENTRIES;
    out.truncate(MAX_TREE_ENTRIES);
    Ok(FileTreeView {
        entries: out,
        truncated,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_path_rejects_escape_and_absolute() {
        assert!(validate_workspace_path("src/main.rs").is_ok());
        assert!(validate_workspace_path("").is_err());
        assert!(validate_workspace_path("/etc/passwd").is_err());
        assert!(validate_workspace_path("../secret").is_err());
        assert!(validate_workspace_path("a/../../b").is_err());
        assert!(validate_workspace_path("a\\b").is_err());
    }
}
