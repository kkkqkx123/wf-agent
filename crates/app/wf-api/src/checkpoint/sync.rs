//! File-checkpoint sync API: export the internal main tree into the user
//! workspace, import external commits back as human history, rebase the
//! baseline, and query reconcile/external-update state. Thin wrappers over
//! `wf_checkpoint::FileCheckpointManager` so transport layers stay free of
//! checkpoint internals.

use std::path::PathBuf;

use wf_checkpoint::file::FileCheckpointManager;
use wf_checkpoint::ExportOptions;

use crate::infra::context::ApiContext;
use crate::ApiError;
use crate::ApiResult;

fn manager(ctx: &ApiContext) -> ApiResult<&FileCheckpointManager> {
    ctx.file_checkpoint_manager().ok_or_else(|| {
        ApiError::execution("file checkpointing is not enabled; set file_checkpoint.enabled=true")
    })
}

fn workspace_root(manager: &FileCheckpointManager) -> ApiResult<PathBuf> {
    manager.workspace_root().map(PathBuf::from).ok_or_else(|| {
        ApiError::Validation("file checkpoint workspace root is not configured".to_string())
    })
}

fn checkpoint_err(e: wf_checkpoint::CheckpointError) -> ApiError {
    use wf_checkpoint::CheckpointError as CE;
    match e {
        CE::Validation { reason } => ApiError::Validation(reason),
        CE::NotFound { id } => ApiError::not_found("file-checkpoint", &id),
        CE::Branch(msg) => ApiError::Conflict(msg),
        CE::MergeConflict { actor, files } => {
            ApiError::Conflict(format!("merge conflict actor={actor} files={files:?}"))
        }
        CE::Storage(e) => ApiError::from(e),
        other => ApiError::execution_with_source(other),
    }
}

pub fn export_to_workspace(
    ctx: &ApiContext,
    confirm_overwrite: bool,
    auto_commit: Option<bool>,
    message: Option<String>,
) -> ApiResult<wf_checkpoint::ExportOutcome> {
    let manager = manager(ctx)?;
    let root = workspace_root(manager)?;
    let opts = ExportOptions {
        confirm_overwrite,
        auto_commit: auto_commit.unwrap_or_else(|| manager.auto_commit_enabled()),
        message,
    };
    manager
        .export_to_workspace(&root, opts)
        .map_err(checkpoint_err)
}

pub fn import_from_external(ctx: &ApiContext) -> ApiResult<wf_checkpoint::ImportOutcome> {
    let manager = manager(ctx)?;
    let root = workspace_root(manager)?;
    manager.import_from_external(&root).map_err(checkpoint_err)
}

pub fn rebase_baseline(
    ctx: &ApiContext,
    baseline: wf_types::config::file_checkpoint::BaselineConfig,
) -> ApiResult<wf_checkpoint::BaselineRebaseOutcome> {
    manager(ctx)?
        .rebase_baseline(&baseline)
        .map_err(checkpoint_err)
}

pub fn external_update_hint(
    ctx: &ApiContext,
) -> ApiResult<Option<wf_checkpoint::ExternalUpdateHint>> {
    let manager = manager(ctx)?;
    let root = workspace_root(manager)?;
    manager.external_update_hint(&root).map_err(checkpoint_err)
}

pub fn reconcile_point(ctx: &ApiContext) -> ApiResult<Option<wf_checkpoint::ReconcilePoint>> {
    manager(ctx)?.reconcile_point().map_err(checkpoint_err)
}

pub fn sync_config(
    ctx: &ApiContext,
) -> ApiResult<Option<wf_types::config::file_checkpoint::SyncConfig>> {
    Ok(manager(ctx)?.sync_config())
}
