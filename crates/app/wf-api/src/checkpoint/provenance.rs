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

/// Begin a new edit session for grouping a multi-file operation.
pub fn begin_session(ctx: &ApiContext, label: Option<String>) -> ApiResult<String> {
    manager(ctx)?
        .begin_session(label)
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
