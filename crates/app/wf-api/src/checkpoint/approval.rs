use wf_checkpoint::approval::{MergeOutcome, PendingApproval};

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

/// All pending layered approvals: actor partitions at the approval layer
/// submitted but neither merged nor rejected. Persisted in Sqlite, so the
/// list survives across executions ("review after the run ends").
pub fn list_pending_approvals(ctx: &ApiContext) -> ApiResult<Vec<PendingApproval>> {
    manager(ctx)?
        .list_pending_approvals()
        .map_err(ApiError::execution_with_source)
}

/// Approve a pending approval: merge the actor's changes into the named
/// feature partition under the configured conflict behavior. `paths`
/// selects file-level approval — when `Some` and non-empty, only the listed
/// files are advanced into the feature and the rest stay pending in the
/// approval layer. Returns the merge outcome (conflicts, marker files,
/// snapshot id).
pub fn approve_changes(
    ctx: &ApiContext,
    agent_instance_id: &str,
    feature_name: &str,
    paths: Option<Vec<String>>,
) -> ApiResult<MergeOutcome> {
    let manager = manager(ctx)?;
    let feature = if feature_name.is_empty() {
        wf_checkpoint::file::FileCheckpointManager::default_feature_name(agent_instance_id)
    } else {
        feature_name.to_string()
    };
    let outcome = match paths {
        Some(paths) if !paths.is_empty() => {
            manager.approve_pending_paths(agent_instance_id, &feature, paths)
        }
        _ => manager.approve_pending(agent_instance_id, &feature),
    };
    outcome.map_err(ApiError::execution_with_source)
}

/// Reject a pending approval: roll the actor's approval partition back to
/// its baseline. Returns the baseline snapshot id (hex).
pub fn reject_changes(ctx: &ApiContext, agent_instance_id: &str) -> ApiResult<String> {
    manager(ctx)?
        .reject_changes(agent_instance_id)
        .map_err(ApiError::execution_with_source)
}
