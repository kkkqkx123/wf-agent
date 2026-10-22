use std::collections::HashSet;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::types::execution_instance::ExecutionKind;
use wf_storage::domain::ExecutionIndexRow;
use wf_types::execution::ExecutionType;
use wf_types::ExecutionStatus;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub struct Placement {
    pub id: String,
    pub kind: ExecutionType,
    pub status: ExecutionStatus,
    /// Absent at a root, as is the engine recorded with it.
    pub parent: Option<ExecutionRef>,
    pub root: ExecutionRef,
    pub depth: u32,
    pub ancestors: Vec<String>,
    /// The execution's own materialised path, which is the prefix selecting it
    /// and everything below it.
    pub path: String,
}

/// Reconcile the answers two sources give about the same executions: the
/// persisted rows are authoritative, and a live entry is taken only where no
/// row covers it, because an execution runs before its record is written.
///
/// Position resolution and the subtree listing both go through this, so they
/// cannot come to opposite conclusions about one execution.
pub(super) fn merge_persisted_then_live<T>(
    persisted: Vec<T>,
    live: Vec<T>,
    id_of: impl Fn(&T) -> String,
) -> Vec<T> {
    let mut merged = persisted;
    let mut seen: HashSet<String> = merged.iter().map(&id_of).collect();
    merged.extend(live.into_iter().filter(|entry| seen.insert(id_of(entry))));
    merged
}

/// Turn one readable indexed row into its position. The row carries the
/// engines of the parent and the root, so the position needs no further read.
fn placement_from_row(row: &ExecutionIndexRow) -> Placement {
    Placement {
        id: row.id().to_string(),
        kind: *row.kind(),
        status: row.status().clone(),
        parent: row.parent().map(|(parent_id, parent_type)| ExecutionRef {
            execution_id: parent_id.to_string(),
            execution_type: *parent_type,
        }),
        root: ExecutionRef {
            execution_id: row.root().to_string(),
            execution_type: row.root_kind(),
        },
        depth: row.depth(),
        ancestors: row.ancestors(),
        path: row.path().to_string(),
    }
}

/// Position of an execution that is still live. Present only when the live
/// entity answers with a hierarchy manager, which is what makes a lineage.
fn live_placement(ctx: &ApiContext, id: &str) -> Option<Placement> {
    let handle = ctx.execution_instance(id)?;
    let manager = handle.hierarchy_manager()?;
    let kind = match handle.kind() {
        ExecutionKind::Agent => ExecutionType::Agent,
        ExecutionKind::Workflow => ExecutionType::Workflow,
    };
    // One constructor builds lineage for the live answer and for the persisted
    // record alike, so the two cannot place one execution differently.
    let lineage = wf_types::execution::ExecutionHierarchy::new(
        id.to_string(),
        id.to_string(),
        manager.ancestors(),
        manager.parent().map(|p| p.parent_type),
        Some(manager.root_execution_type()),
        manager.fork_path(),
    );
    Some(Placement {
        id: id.to_string(),
        kind,
        status: handle.status().into(),
        parent: manager.parent().map(|p| ExecutionRef {
            execution_id: p.parent_id.to_string(),
            execution_type: p.parent_type,
        }),
        root: ExecutionRef {
            execution_id: lineage.root_execution_id(),
            execution_type: manager.root_execution_type(),
        },
        depth: lineage.depth(),
        ancestors: lineage.ancestors(),
        path: lineage.path().to_string(),
    })
}

pub(super) async fn placement(ctx: &ApiContext, id: &str) -> ApiResult<Option<Placement>> {
    let mut persisted = Vec::new();
    for kind in KINDS {
        if let Some(row) = super::scan::read_row(ctx, &kind, id).await? {
            persisted.push(placement_from_row(&row));
            break;
        }
    }
    let live: Vec<Placement> = live_placement(ctx, id).into_iter().collect();
    Ok(merge_persisted_then_live(persisted, live, |p| p.id.clone()).pop())
}

