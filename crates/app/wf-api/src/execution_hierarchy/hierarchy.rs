use wf_types::execution::ExecutionType;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

use super::*;

pub async fn execution_type(ctx: &ApiContext, id: &str) -> ApiResult<ExecutionType> {
    let placement = super::placement::placement(ctx, id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(id))?;
    Ok(placement.kind)
}

/// Hierarchy position of one execution.
///
/// The parent's and the root's engines come off the same row that gives this
/// execution's own position, so the answer costs that one row and nothing else.
pub async fn hierarchy(ctx: &ApiContext, id: &str) -> ApiResult<ExecutionHierarchyView> {
    let placement = super::placement::placement(ctx, id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(id))?;
    Ok(ExecutionHierarchyView {
        execution_id: placement.id,
        execution_type: placement.kind,
        status: placement.status,
        depth: placement.depth,
        parent: placement.parent,
        root: placement.root,
        ancestors: placement.ancestors,
    })
}

