use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::types::execution_instance::ExecutionKind;
use wf_storage::domain::ExecutionIndexRow;
use wf_types::execution::ExecutionType;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

use super::*;

pub async fn subtree(
    ctx: &ApiContext,
    root_id: &str,
    limit: usize,
    offset: usize,
) -> ApiResult<ExecutionSubtree> {
    // The queried node's own path is the prefix that selects it and everything
    // below, so a nested node answers the same way a root does.
    let root = super::placement::placement(ctx, root_id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(root_id))?;

    let (rows, rejected) =
        super::scan::scan_all_rows(ctx, &ExecutionIndexRow::path_filter(&root.path)).await?;
    let persisted: Vec<ExecutionSubtreeNode> = rows
        .iter()
        .map(|row| ExecutionSubtreeNode {
            execution_id: row.id().to_string(),
            execution_type: *row.kind(),
            status: Some(row.status().clone()),
            depth: depth_within(row, root.depth),
            parent_execution_id: row.parent().map(|(parent_id, _)| parent_id.to_string()),
        })
        .collect();

    let live: Vec<ExecutionSubtreeNode> = ctx
        .execution_subtree(root_id)
        .into_iter()
        .filter_map(|handle| {
            let manager = handle.hierarchy_manager()?;
            Some(ExecutionSubtreeNode {
                execution_id: handle.id().to_string(),
                execution_type: match handle.kind() {
                    ExecutionKind::Agent => ExecutionType::Agent,
                    ExecutionKind::Workflow => ExecutionType::Workflow,
                },
                status: Some(handle.status().into()),
                depth: manager.depth().saturating_sub(root.depth),
                parent_execution_id: manager.parent_id().map(|p| p.to_string()),
            })
        })
        .collect();

    let mut nodes =
        super::placement::merge_persisted_then_live(persisted, live, |n| n.execution_id.clone());
    nodes.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.execution_id.cmp(&b.execution_id))
    });
    let total = nodes.len();
    let start = offset.min(total);
    let window: Vec<ExecutionSubtreeNode> = nodes
        .into_iter()
        .skip(start)
        .take(limit.saturating_add(1))
        .collect();
    let has_more = window.len() > limit;
    let mut window = window;
    window.truncate(limit);
    let end = start.saturating_add(window.len());
    let omitted = total.saturating_sub(end);
    Ok(ExecutionSubtree {
        root_execution_id: root_id.to_string(),
        truncated: has_more,
        omitted,
        next_offset: has_more.then_some(end as u64),
        nodes: window,
        rejected: super::scan::rejected_rows(rejected),
    })
}

/// Nesting level of a row relative to the queried root, which is what the
/// listing reports as a node's depth.
fn depth_within(row: &ExecutionIndexRow, root_depth: u32) -> u32 {
    row.depth().saturating_sub(root_depth)
}
