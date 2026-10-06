//! Parent/child queries over the unified execution handle.
//!
//! A live execution answers through the hierarchy manager its entity owns; a
//! persisted workflow or agent record answers from its indexed storage
//! metadata, so a terminated run keeps reporting its place in the tree after
//! the live registry drops it.
//!
//! Every link is stored forward — an execution names its own parent and root
//! — so the shape of the tree is recovered by querying those fields. Nothing
//! here follows a child list cached on the parent, which is why a tree stays
//! correct across a restart and mid-run parent persists.

use serde::Serialize;
use std::collections::HashSet;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::types::execution_instance::ExecutionKind;
use wf_storage::domain::store::QueryFilter;
use wf_storage::domain::ExecutionIndexError;
use wf_storage::domain::ExecutionIndexRow;
use wf_storage::error::StorageError;
use wf_types::execution::ExecutionType;
use wf_types::ExecutionStatus;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Node cap for one subtree query. A wide hierarchy would otherwise make a
/// single request unbounded; the truncation is reported instead of silent.
///
/// The cap guards response size only: the prefix scan already returns every
/// matching row, so clipping costs no query work. There is no continuation
/// cursor, because a caller that needs more asks for a descendant, whose own
/// subtree is a strictly narrower answer.
pub const MAX_SUBTREE_NODES: usize = 512;

/// One execution as referenced from a hierarchy view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExecutionRef {
    pub execution_id: String,
    pub execution_type: ExecutionType,
}

/// Where one execution sits in the parent/child tree.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionHierarchyView {
    pub execution_id: String,
    pub execution_type: ExecutionType,
    pub status: ExecutionStatus,
    /// Nesting level below the root; a root execution is `0`.
    pub depth: u32,
    /// Absent for a root execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<ExecutionRef>,
    /// The execution this one descends from; equals `execution_id` at the root.
    pub root: ExecutionRef,
    /// Root-to-parent id chain, oldest first, excluding this execution.
    pub ancestors: Vec<String>,
}

/// One node of a subtree listing.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionSubtreeNode {
    pub execution_id: String,
    pub execution_type: ExecutionType,
    /// The node's own status, taken from the live handle when there is one and
    /// from the persisted row otherwise. `None` only for a live execution whose
    /// record has not been written yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ExecutionStatus>,
    /// Nesting level relative to the queried root, which is `0`.
    pub depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_execution_id: Option<String>,
}

/// Every execution below a root, breadth-first.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionSubtree {
    pub root_execution_id: String,
    /// Set when the node cap dropped descendants, so a caller can tell a
    /// complete tree from a clipped one.
    pub truncated: bool,
    /// How many descendants the node cap dropped. Query a descendant for the
    /// part of the tree this response left out.
    #[serde(skip_serializing_if = "is_zero")]
    pub omitted: usize,
    /// Root first, then each level in child order.
    pub nodes: Vec<ExecutionSubtreeNode>,
    /// Rows this scan reached but could not read. Reported so a caller can
    /// tell a complete tree from one with an unreadable node in it, and can
    /// name the record to look at.
    pub rejected: Vec<RejectedRow>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// Both execution record kinds share one id space, so a hierarchy query has to
/// consult both and reconcile the rows.
const KINDS: [ExecutionType; 2] = [ExecutionType::Workflow, ExecutionType::AgentLoop];

/// One persisted row that could not be interpreted.
///
/// Carried beside the rows that could, so a single record whose indexed
/// metadata is absent or self-contradictory is reported to the caller instead
/// of failing the query for every other node in the tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RejectedRow {
    pub execution_id: String,
    pub reason: String,
}

/// Read the indexed rows matching `filter` from the store of one entity kind,
/// keeping the rows that fail to parse beside the ones that parse. Every row
/// is attempted, so one broken record is named rather than allowed to end the
/// scan for its neighbours.
async fn scan_rows(
    ctx: &ApiContext,
    kind: &ExecutionType,
    filter: &QueryFilter,
) -> ApiResult<(Vec<ExecutionIndexRow>, Vec<(String, ExecutionIndexError)>)> {
    let raw = match kind {
        ExecutionType::Workflow => {
            ctx.storage
                .workflow_execution
                .entity_store()
                .list_metadata(Some(filter))
                .await?
        }
        ExecutionType::AgentLoop => {
            ctx.storage
                .agent_execution
                .entity_store()
                .list_metadata(Some(filter))
                .await?
        }
    };
    let mut rows = Vec::new();
    let mut rejected = Vec::new();
    for (id, meta) in raw {
        match ExecutionIndexRow::parse(id.clone(), kind.clone(), &meta) {
            Ok(row) => rows.push(row),
            Err(err) => rejected.push((id, err)),
        }
    }
    Ok((rows, rejected))
}

/// Read the indexed rows of every execution matching `filter`, across both
/// record kinds.
async fn scan_all_rows(
    ctx: &ApiContext,
    filter: &QueryFilter,
) -> ApiResult<(Vec<ExecutionIndexRow>, Vec<(String, ExecutionIndexError)>)> {
    let mut rows = Vec::new();
    let mut rejected = Vec::new();
    for kind in KINDS {
        let (kind_rows, kind_rejected) = scan_rows(ctx, &kind, filter).await?;
        rows.extend(kind_rows);
        rejected.extend(kind_rejected);
    }
    Ok((rows, rejected))
}

/// Turn rows a scan could not read into their reported form.
fn rejected_rows(rejected: Vec<(String, ExecutionIndexError)>) -> Vec<RejectedRow> {
    rejected
        .into_iter()
        .map(|(execution_id, err)| RejectedRow {
            reason: err.to_string(),
            execution_id,
        })
        .collect()
}

/// Read the rows of one entity kind for a query whose answer is about a single
/// execution whose own record has to be readable. An unreadable row is then
/// the failure rather than an omission, and reports as a conflict naming the
/// record.
async fn read_rows(
    ctx: &ApiContext,
    kind: &ExecutionType,
    filter: &QueryFilter,
) -> ApiResult<Vec<ExecutionIndexRow>> {
    let (rows, rejected) = scan_rows(ctx, kind, filter).await?;
    if let Some((_, err)) = rejected.into_iter().next() {
        return Err(StorageError::from(err).into());
    }
    Ok(rows)
}

/// Read one execution's indexed row from the store of the given kind. A miss
/// means the record is absent, not that it lacks a hierarchy.
async fn read_row(
    ctx: &ApiContext,
    kind: &ExecutionType,
    id: &str,
) -> ApiResult<Option<ExecutionIndexRow>> {
    let filter = QueryFilter::new().with_id(id);
    Ok(read_rows(ctx, kind, &filter)
        .await?
        .into_iter()
        .find(|row| row.id() == id))
}

/// Where one execution sits.
struct Placement {
    id: String,
    kind: ExecutionType,
    status: ExecutionStatus,
    /// Absent at a root, as is the engine recorded with it.
    parent: Option<ExecutionRef>,
    root: ExecutionRef,
    depth: u32,
    ancestors: Vec<String>,
    /// The execution's own materialised path, which is the prefix selecting it
    /// and everything below it.
    path: String,
}

/// Reconcile the answers two sources give about the same executions: the
/// persisted rows are authoritative, and a live entry is taken only where no
/// row covers it, because an execution runs before its record is written.
///
/// Position resolution and the subtree listing both go through this, so they
/// cannot come to opposite conclusions about one execution.
fn merge_persisted_then_live<T>(
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
        kind: row.kind().clone(),
        status: row.status().clone(),
        parent: row.parent().map(|(parent_id, parent_type)| ExecutionRef {
            execution_id: parent_id.to_string(),
            execution_type: parent_type.clone(),
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
        ExecutionKind::Agent => ExecutionType::AgentLoop,
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

async fn placement(ctx: &ApiContext, id: &str) -> ApiResult<Option<Placement>> {
    let mut persisted = Vec::new();
    for kind in KINDS {
        if let Some(row) = read_row(ctx, &kind, id).await? {
            persisted.push(placement_from_row(&row));
            break;
        }
    }
    let live: Vec<Placement> = live_placement(ctx, id).into_iter().collect();
    Ok(merge_persisted_then_live(persisted, live, |p| p.id.clone()).pop())
}

/// The engine that owns an execution, live or persisted. Queries that need
/// to branch on the domain read it from here instead of re-resolving the id.
pub async fn execution_type(ctx: &ApiContext, id: &str) -> ApiResult<ExecutionType> {
    let placement = placement(ctx, id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(id))?;
    Ok(placement.kind)
}

/// Hierarchy position of one execution.
///
/// The parent's and the root's engines come off the same row that gives this
/// execution's own position, so the answer costs that one row and nothing else.
pub async fn hierarchy(ctx: &ApiContext, id: &str) -> ApiResult<ExecutionHierarchyView> {
    let placement = placement(ctx, id)
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

/// Every execution in the tree rooted at `root_id`.
///
/// One prefix scan per record kind covers every persisted node; the live
/// managers are then walked in memory and the two sets reconciled by
/// [`merge_persisted_then_live`]. Neither step issues a query per node, so the
/// cost does not grow with the width of the tree.
pub async fn subtree(ctx: &ApiContext, root_id: &str) -> ApiResult<ExecutionSubtree> {
    // The queried node's own path is the prefix that selects it and everything
    // below, so a nested node answers the same way a root does.
    let root = placement(ctx, root_id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(root_id))?;

    let (rows, rejected) = scan_all_rows(ctx, &ExecutionIndexRow::path_filter(&root.path)).await?;
    let persisted: Vec<ExecutionSubtreeNode> = rows
        .iter()
        .map(|row| ExecutionSubtreeNode {
            execution_id: row.id().to_string(),
            execution_type: row.kind().clone(),
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
                    ExecutionKind::Agent => ExecutionType::AgentLoop,
                    ExecutionKind::Workflow => ExecutionType::Workflow,
                },
                status: Some(handle.status().into()),
                depth: manager.depth().saturating_sub(root.depth),
                parent_execution_id: manager.parent_id().map(|p| p.to_string()),
            })
        })
        .collect();

    let mut nodes = merge_persisted_then_live(persisted, live, |n| n.execution_id.clone());
    nodes.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.execution_id.cmp(&b.execution_id))
    });
    let total = nodes.len();
    if total > MAX_SUBTREE_NODES {
        nodes.truncate(MAX_SUBTREE_NODES);
    }
    let omitted = total - nodes.len();
    Ok(ExecutionSubtree {
        root_execution_id: root_id.to_string(),
        truncated: omitted > 0,
        omitted,
        nodes,
        rejected: rejected_rows(rejected),
    })
}

/// Nesting level of a row relative to the queried root, which is what the
/// listing reports as a node's depth.
fn depth_within(row: &ExecutionIndexRow, root_depth: u32) -> u32 {
    row.depth().saturating_sub(root_depth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_agent::entity::AgentLoopEntity;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::execution::ExecutionHierarchy;
    use wf_types::Id;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    fn manager(id: &str, kind: ExecutionType) -> Arc<wf_core::ExecutionHierarchyManager> {
        Arc::new(wf_core::ExecutionHierarchyManager::new(
            Id::from(id.to_string()),
            kind,
        ))
    }

    async fn register_agent(
        ctx: &ApiContext,
        id: &str,
        hierarchy: Arc<wf_core::ExecutionHierarchyManager>,
    ) {
        let entity = Arc::new(
            AgentLoopEntity::new(Id::from(id.to_string())).with_hierarchy_manager(hierarchy),
        );
        let _ = ctx.agent_loops.register(entity);
    }

    /// The persisted form of a manager: the same forward links the engines
    /// write onto their execution records.
    fn persisted_hierarchy(
        manager: &Arc<wf_core::ExecutionHierarchyManager>,
    ) -> Option<ExecutionHierarchy> {
        let parent = manager.parent();
        let ancestors = manager.ancestors();
        if parent.is_none() && ancestors.is_empty() && manager.fork_path().is_none() {
            return None;
        }
        Some(ExecutionHierarchy::new(
            manager.execution_id(),
            manager.execution_id(),
            ancestors,
            parent.as_ref().map(|p| p.parent_type.clone()),
            Some(manager.root_execution_type()),
            manager.fork_path(),
        ))
    }

    fn workflow_record(
        id: &str,
        hierarchy: Option<ExecutionHierarchy>,
    ) -> wf_types::WorkflowExecution {
        wf_types::WorkflowExecution {
            id: id.to_string(),
            workflow_id: "wf-1".to_string(),
            workflow_version: None,
            status: ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: None,
            error: None,
            started_at: 10,
            completed_at: Some(20),
            execution_type: None,
            fork_join_context: None,
            hierarchy,
        }
    }

    fn agent_record(id: &str, hierarchy: Option<ExecutionHierarchy>) -> wf_types::AgentExecution {
        wf_types::AgentExecution {
            id: Id::from(id.to_string()),
            definition_id: Id::from("agent-1".to_string()),
            status: ExecutionStatus::Failed,
            current_iteration: 3,
            tool_call_count: 2,
            iteration_history: None,
            started_at: 5,
            completed_at: Some(15),
            error: Some("boom".to_string()),
            context: None,
            loop_config: None,
            permanently_failed_tools: None,
            hierarchy,
        }
    }

    #[tokio::test]
    async fn live_root_reports_no_parent() {
        let ctx = make_ctx();
        register_agent(
            &ctx,
            "live-root",
            manager("live-root", ExecutionType::AgentLoop),
        )
        .await;

        let view = hierarchy(&ctx, "live-root").await.unwrap();
        assert_eq!(view.execution_id, "live-root");
        assert_eq!(view.execution_type, ExecutionType::AgentLoop);
        assert_eq!(view.depth, 0);
        assert!(view.parent.is_none());
        assert_eq!(view.root.execution_id, "live-root");
        assert!(view.ancestors.is_empty());
    }

    #[tokio::test]
    async fn live_child_reports_depth_root_and_parent() {
        let ctx = make_ctx();
        let root = manager("root", ExecutionType::AgentLoop);
        let child = root
            .derive_child(Id::from("kid"), ExecutionType::AgentLoop, None)
            .unwrap();
        register_agent(&ctx, "root", root).await;
        register_agent(&ctx, "kid", child).await;

        let view = hierarchy(&ctx, "kid").await.unwrap();
        assert_eq!(view.depth, 1);
        let parent = view.parent.expect("a derived child has a parent");
        assert_eq!(parent.execution_id, "root");
        assert_eq!(parent.execution_type, ExecutionType::AgentLoop);
        assert_eq!(view.root.execution_id, "root");
        assert_eq!(view.ancestors, vec!["root".to_string()]);
    }

    #[tokio::test]
    async fn subtree_walks_live_children_breadth_first() {
        let ctx = make_ctx();
        let root = manager("root", ExecutionType::Workflow);
        let a = root
            .derive_child(Id::from("a"), ExecutionType::AgentLoop, None)
            .unwrap();
        let b = root
            .derive_child(Id::from("b"), ExecutionType::Workflow, None)
            .unwrap();
        let a1 = a
            .derive_child(Id::from("a1"), ExecutionType::AgentLoop, None)
            .unwrap();
        register_agent(&ctx, "root", root).await;
        register_agent(&ctx, "a", a).await;
        register_agent(&ctx, "b", b).await;
        register_agent(&ctx, "a1", a1).await;

        let tree = subtree(&ctx, "root").await.unwrap();
        assert!(!tree.truncated);
        let order: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
        assert_eq!(order, vec!["root", "a", "b", "a1"]);
        let depths: Vec<u32> = tree.nodes.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1, 1, 2]);
        assert_eq!(tree.nodes[0].parent_execution_id, None);
        assert_eq!(tree.nodes[3].parent_execution_id.as_deref(), Some("a"));
    }

    /// A subtree listing must cost the same number of metadata reads whatever
    /// the width of the tree: one prefix scan per record kind, and the live
    /// part walked in memory. A per-node children lookup would make the wide
    /// tree cost proportionally more.
    #[tokio::test]
    async fn subtree_reads_do_not_grow_with_the_number_of_nodes() {
        async fn reads_for(ctx: &ApiContext, child_count: usize) -> usize {
            persisted_star(ctx, child_count).await;
            for backend in ctx.storage.all_backends() {
                if let Some(memory) = backend.memory_storage() {
                    memory.reset_read_count();
                }
            }
            let tree = subtree(ctx, "r").await.unwrap();
            assert_eq!(tree.nodes.len(), child_count + 1);
            ctx.storage
                .workflow_execution
                .store()
                .memory_storage()
                .unwrap()
                .read_count()
        }

        let narrow = make_ctx();
        let wide = make_ctx();
        assert_eq!(
            reads_for(&narrow, 1).await,
            reads_for(&wide, 20).await,
            "a wide tree must not cost more metadata reads than a narrow one"
        );
    }

    #[tokio::test]
    async fn persisted_records_answer_without_a_live_entity() {
        let ctx = make_ctx();
        let root_manager = manager("wf-root", ExecutionType::Workflow);
        let child_manager = root_manager
            .derive_child(Id::from("agent-kid"), ExecutionType::AgentLoop, None)
            .unwrap();

        ctx.storage
            .workflow_execution
            .save(&workflow_record(
                "wf-root",
                persisted_hierarchy(&root_manager),
            ))
            .await
            .unwrap();
        ctx.storage
            .agent_execution
            .save(&agent_record(
                "agent-kid",
                persisted_hierarchy(&child_manager),
            ))
            .await
            .unwrap();

        let view = hierarchy(&ctx, "agent-kid").await.unwrap();
        assert_eq!(view.execution_type, ExecutionType::AgentLoop);
        assert_eq!(view.status, ExecutionStatus::Failed);
        assert_eq!(view.depth, 1);
        assert_eq!(view.root.execution_id, "wf-root");
        assert_eq!(view.ancestors, vec!["wf-root".to_string()]);
        assert_eq!(view.parent.unwrap().execution_id, "wf-root");

        let tree = subtree(&ctx, "wf-root").await.unwrap();
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[1].execution_type, ExecutionType::AgentLoop);
    }

    /// A parent that last persisted before its children existed still reports
    /// them, because the child records carry the link rather than the parent
    /// carrying a snapshot of its children.
    #[tokio::test]
    async fn tree_survives_a_parent_that_predates_its_children() {
        let ctx = make_ctx();
        let root_manager = manager("wf-root", ExecutionType::Workflow);
        let child_manager = root_manager
            .derive_child(Id::from("agent-kid"), ExecutionType::AgentLoop, None)
            .unwrap();

        // The parent record is written from the manager as it looked before
        // the child was spawned: no parent, no ancestors, no fork path.
        ctx.storage
            .workflow_execution
            .save(&workflow_record(
                "wf-root",
                persisted_hierarchy(&root_manager),
            ))
            .await
            .unwrap();
        // The child record is written afterwards and links forward.
        ctx.storage
            .agent_execution
            .save(&agent_record(
                "agent-kid",
                persisted_hierarchy(&child_manager),
            ))
            .await
            .unwrap();

        let tree = subtree(&ctx, "wf-root").await.unwrap();
        assert_eq!(tree.nodes.len(), 2, "the child is still reachable");
        assert_eq!(tree.nodes[1].execution_id, "agent-kid");
    }

    #[tokio::test]
    async fn a_root_is_never_its_own_child() {
        let ctx = make_ctx();
        ctx.storage
            .workflow_execution
            .save(&workflow_record("solo", None))
            .await
            .unwrap();

        let view = hierarchy(&ctx, "solo").await.unwrap();
        assert!(view.parent.is_none());
        assert_eq!(view.root.execution_id, "solo");

        let tree = subtree(&ctx, "solo").await.unwrap();
        assert_eq!(tree.nodes.len(), 1, "a root lists only itself");
        assert_eq!(tree.nodes[0].execution_id, "solo");
        assert!(tree.nodes[0].parent_execution_id.is_none());
    }

    #[tokio::test]
    async fn ancestors_span_a_three_level_persisted_chain() {
        let ctx = make_ctx();
        let root = manager("r", ExecutionType::Workflow);
        let child = root
            .derive_child(Id::from("c"), ExecutionType::AgentLoop, None)
            .unwrap();
        let grandchild = child
            .derive_child(Id::from("g"), ExecutionType::AgentLoop, None)
            .unwrap();

        for (id, mgr) in [("r", &root), ("c", &child), ("g", &grandchild)] {
            ctx.storage
                .agent_execution
                .save(&agent_record(id, persisted_hierarchy(mgr)))
                .await
                .unwrap();
        }

        let view = hierarchy(&ctx, "g").await.unwrap();
        assert_eq!(view.depth, 2);
        assert_eq!(view.ancestors, vec!["r".to_string(), "c".to_string()]);
        assert_eq!(view.parent.unwrap().execution_id, "c");
        assert_eq!(view.root.execution_id, "r");
    }

    /// Root `r` has two children, `a` and `b`, and `g` hangs under `a`. An
    /// ancestor query that answered "everything sharing my root at a shallower
    /// depth" would hand back `b` here, so this shape is the one a chain-only
    /// fixture cannot cover.
    /// A root with `child_count` direct children: the shape both the read
    /// cost and the node cap tests need.
    async fn persisted_star(ctx: &ApiContext, child_count: usize) {
        let root = manager("r", ExecutionType::Workflow);
        ctx.storage
            .workflow_execution
            .save(&workflow_record("r", persisted_hierarchy(&root)))
            .await
            .unwrap();
        for i in 0..child_count {
            let child = root
                .derive_child(Id::from(format!("c{i}")), ExecutionType::AgentLoop, None)
                .unwrap();
            ctx.storage
                .agent_execution
                .save(&agent_record(&format!("c{i}"), persisted_hierarchy(&child)))
                .await
                .unwrap();
        }
    }

    async fn branching_tree(ctx: &ApiContext) {
        let root = manager("r", ExecutionType::Workflow);
        let a = root
            .derive_child(Id::from("a"), ExecutionType::AgentLoop, None)
            .unwrap();
        let b = root
            .derive_child(Id::from("b"), ExecutionType::AgentLoop, None)
            .unwrap();
        let g = a
            .derive_child(Id::from("g"), ExecutionType::AgentLoop, None)
            .unwrap();

        ctx.storage
            .workflow_execution
            .save(&workflow_record("r", persisted_hierarchy(&root)))
            .await
            .unwrap();
        for (id, mgr) in [("a", &a), ("b", &b), ("g", &g)] {
            ctx.storage
                .agent_execution
                .save(&agent_record(id, persisted_hierarchy(mgr)))
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn a_sibling_is_never_reported_as_an_ancestor() {
        let ctx = make_ctx();
        branching_tree(&ctx).await;

        let view = hierarchy(&ctx, "g").await.unwrap();
        assert_eq!(view.ancestors, vec!["r".to_string(), "a".to_string()]);
        assert!(
            !view.ancestors.contains(&"b".to_string()),
            "b shares a root and a shallower depth but is not an ancestor"
        );
        assert_eq!(view.depth, 2);
        assert_eq!(view.root.execution_id, "r");
    }

    #[tokio::test]
    async fn one_prefix_scan_covers_a_tree_split_across_both_engines() {
        let ctx = make_ctx();
        branching_tree(&ctx).await;

        let tree = subtree(&ctx, "r").await.unwrap();
        assert!(!tree.truncated);
        assert_eq!(tree.omitted, 0);
        let order: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
        assert_eq!(order, vec!["r", "a", "b", "g"]);
        let depths: Vec<u32> = tree.nodes.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1, 1, 2]);
        assert_eq!(tree.nodes[3].parent_execution_id.as_deref(), Some("a"));
    }

    #[tokio::test]
    async fn a_subtree_is_scoped_to_the_queried_node() {
        let ctx = make_ctx();
        branching_tree(&ctx).await;

        let tree = subtree(&ctx, "a").await.unwrap();
        let order: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
        assert_eq!(order, vec!["a", "g"]);
        let depths: Vec<u32> = tree.nodes.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1]);
    }

    #[tokio::test]
    async fn a_root_named_alike_does_not_join_another_tree() {
        let ctx = make_ctx();
        branching_tree(&ctx).await;
        ctx.storage
            .workflow_execution
            .save(&workflow_record("rx", None))
            .await
            .unwrap();

        let tree = subtree(&ctx, "r").await.unwrap();
        let order: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
        assert!(!order.contains(&"rx"), "`/r/` must not match `/rx/`");
    }

    /// A live parent knows the child it spawned a moment ago; a persisted one
    /// knows the children of earlier runs. Neither source is a superset, so
    /// dropping either would hide real children.
    #[tokio::test]
    async fn direct_children_merge_live_and_persisted() {
        let ctx = make_ctx();
        let root = manager("live-root", ExecutionType::AgentLoop);
        let live_child = root
            .derive_child(Id::from("live-kid"), ExecutionType::AgentLoop, None)
            .unwrap();
        let stored_child = root
            .derive_child(Id::from("stored-kid"), ExecutionType::AgentLoop, None)
            .unwrap();
        register_agent(&ctx, "live-root", root).await;
        register_agent(&ctx, "live-kid", live_child).await;
        ctx.storage
            .agent_execution
            .save(&agent_record(
                "stored-kid",
                persisted_hierarchy(&stored_child),
            ))
            .await
            .unwrap();

        let tree = subtree(&ctx, "live-root").await.unwrap();
        let ids: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
        assert_eq!(ids, vec!["live-root", "live-kid", "stored-kid"]);
    }

    #[tokio::test]
    async fn a_live_branch_with_no_records_yet_still_appears() {
        let ctx = make_ctx();
        let root = manager("run", ExecutionType::AgentLoop);
        let child = root
            .derive_child(Id::from("agent-kid"), ExecutionType::AgentLoop, None)
            .unwrap();
        register_agent(&ctx, "run", root).await;
        register_agent(&ctx, "agent-kid", child).await;

        let tree = subtree(&ctx, "run").await.unwrap();
        let order: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
        assert_eq!(order, vec!["run", "agent-kid"]);
    }

    #[tokio::test]
    async fn unknown_id_is_not_found() {
        let ctx = make_ctx();
        assert!(hierarchy(&ctx, "nope").await.is_err());
        assert!(subtree(&ctx, "nope").await.is_err());
    }

    /// The node cap guards response size only, so crossing it drops the
    /// tail of the listing and reports how much went missing rather than
    /// failing or silently returning a shorter tree.
    #[tokio::test]
    async fn a_subtree_reports_how_many_nodes_the_cap_dropped() {
        let ctx = make_ctx();
        persisted_star(&ctx, MAX_SUBTREE_NODES).await;

        let tree = subtree(&ctx, "r").await.unwrap();
        assert_eq!(tree.nodes.len(), MAX_SUBTREE_NODES);
        assert!(tree.truncated);
        assert_eq!(tree.omitted, 1);
        assert!(tree.rejected.is_empty());
    }

    /// One record whose path names a different execution sits inside the
    /// scanned prefix but cannot be read. It is reported, the nodes that do
    /// parse still answer, and reading that record on its own is a conflict
    /// naming it rather than an internal failure the caller would retry.
    #[tokio::test]
    async fn one_broken_record_is_reported_and_the_tree_stays_readable() {
        let ctx = make_ctx();
        branching_tree(&ctx).await;

        let broken = ExecutionHierarchy::new(
            Id::from("wf-1".to_string()),
            Id::from("ghost".to_string()),
            vec![Id::from("r".to_string())],
            Some(ExecutionType::Workflow),
            Some(ExecutionType::Workflow),
            None,
        );
        ctx.storage
            .agent_execution
            .save(&agent_record("mismatch", Some(broken)))
            .await
            .unwrap();

        let tree = subtree(&ctx, "r").await.unwrap();
        let ids: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
        assert_eq!(ids, vec!["r", "a", "b", "g"], "good rows still answer");
        assert_eq!(tree.rejected.len(), 1);
        assert_eq!(tree.rejected[0].execution_id, "mismatch");
        assert!(!tree.rejected[0].reason.is_empty());

        let err = hierarchy(&ctx, "mismatch").await.unwrap_err();
        assert!(
            matches!(err, ApiError::Conflict(_)),
            "a self-contradictory record is a conflict: {err:?}"
        );
    }

    /// The live entity and its persisted row answer for the same execution
    /// with different statuses. The row is authoritative on every read path,
    /// and a path that only the live source can answer still reads the type
    /// both sources agree on.
    #[tokio::test]
    async fn a_persisted_row_wins_on_every_read_path() {
        let ctx = make_ctx();
        let root = manager("r", ExecutionType::Workflow);
        let child = root
            .derive_child(Id::from("kid"), ExecutionType::AgentLoop, None)
            .unwrap();
        ctx.storage
            .workflow_execution
            .save(&workflow_record("r", persisted_hierarchy(&root)))
            .await
            .unwrap();
        ctx.storage
            .agent_execution
            .save(&agent_record("kid", persisted_hierarchy(&child)))
            .await
            .unwrap();
        register_agent(&ctx, "r", root).await;
        register_agent(&ctx, "kid", child).await;

        // The two sources really do disagree, so the assertions below are not
        // satisfied by both saying the same thing.
        let live: ExecutionStatus = ctx.live_execution_status("kid").unwrap().into();
        assert_ne!(live, ExecutionStatus::Failed);

        let view = hierarchy(&ctx, "kid").await.unwrap();
        assert_eq!(view.status, ExecutionStatus::Failed);
        assert_eq!(
            execution_type(&ctx, "kid").await.unwrap(),
            ExecutionType::AgentLoop
        );

        let tree = subtree(&ctx, "r").await.unwrap();
        let node = tree
            .nodes
            .iter()
            .find(|n| n.execution_id == "kid")
            .expect("kid is in the tree of its own root");
        assert_eq!(node.status, Some(ExecutionStatus::Failed));
        assert_eq!(node.execution_type, view.execution_type);
        assert_eq!(
            node.parent_execution_id.as_deref(),
            view.parent
                .as_ref()
                .map(|parent| parent.execution_id.as_str())
        );
    }
}
