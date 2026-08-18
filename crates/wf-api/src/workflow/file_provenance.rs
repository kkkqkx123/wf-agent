use wf_checkpoint::provenance::{DeltaSummary, FileDiffView, PartitionView, WorkspaceFile};

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
