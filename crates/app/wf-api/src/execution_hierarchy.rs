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
use std::collections::{HashSet, VecDeque};
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::types::execution_instance::ExecutionKind;
use wf_storage::domain::store::QueryFilter;
use wf_storage::domain::ExecutionIndexRow;
use wf_storage::error::StorageError;
use wf_types::execution::ExecutionType;
use wf_types::ExecutionStatus;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Node cap for one subtree query. A wide hierarchy would otherwise make a
/// single request unbounded; the truncation is reported instead of silent.
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
    pub children: Vec<ExecutionRef>,
}

/// One node of a subtree listing.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionSubtreeNode {
    pub execution_id: String,
    pub execution_type: ExecutionType,
    /// `None` when the child is live but its record has not been written yet,
    /// or when the child record is gone.
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
    /// How many descendants the cap dropped, so a clipped tree can be resumed
    /// rather than merely detected.
    #[serde(skip_serializing_if = "is_zero")]
    pub omitted: usize,
    /// Root first, then each level in child order.
    pub nodes: Vec<ExecutionSubtreeNode>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// Both execution record kinds share one id space, so a hierarchy query has to
/// consult both and reconcile the rows.
const KINDS: [ExecutionType; 2] = [ExecutionType::Workflow, ExecutionType::AgentLoop];

/// Read the indexed rows matching `filter` from the store of one entity kind.
/// Every row is parsed, so a record whose hierarchy metadata is absent or
/// self-contradictory fails the query instead of dropping out of the tree.
async fn read_rows(
    ctx: &ApiContext,
    kind: &ExecutionType,
    filter: &QueryFilter,
) -> ApiResult<Vec<ExecutionIndexRow>> {
    let raw = match kind {
        ExecutionType::Workflow => ctx
            .storage
            .workflow_execution
            .entity_store()
            .list_metadata(Some(filter))
            .await?,
        ExecutionType::AgentLoop => ctx
            .storage
            .agent_execution
            .entity_store()
            .list_metadata(Some(filter))
            .await?,
    };
    raw.into_iter()
        .map(|(id, meta)| Ok(ExecutionIndexRow::parse(id, &meta).map_err(StorageError::from)?))
        .collect()
}

/// Read the indexed rows of every execution matching `filter`, across both
/// record kinds.
async fn read_all_rows(ctx: &ApiContext, filter: &QueryFilter) -> ApiResult<Vec<ExecutionIndexRow>> {
    let mut rows = Vec::new();
    for kind in KINDS {
        rows.extend(read_rows(ctx, &kind, filter).await?);
    }
    Ok(rows)
}

/// Read one execution's indexed row from the store of the given kind. A miss
/// means the record is absent, not that it lacks a hierarchy.
async fn read_row(ctx: &ApiContext, kind: &ExecutionType, id: &str) -> ApiResult<Option<ExecutionIndexRow>> {
    let filter = QueryFilter::new().with_id(id);
    Ok(read_rows(ctx, kind, &filter)
        .await?
        .into_iter()
        .find(|row| row.id() == id))
}

/// Where one execution sits, taking the live registry first so a running
/// execution reports its in-memory position, then the persisted rows.
struct Placement {
    kind: ExecutionType,
    status: ExecutionStatus,
    parent: Option<String>,
    root: String,
    depth: u32,
    ancestors: Vec<String>,
}

async fn placement(ctx: &ApiContext, id: &str) -> ApiResult<Option<Placement>> {
    if let Some(handle) = ctx.execution_instance(id) {
        let kind = match handle.kind() {
            ExecutionKind::Agent => ExecutionType::AgentLoop,
            ExecutionKind::Workflow => ExecutionType::Workflow,
        };
        if let Some(manager) = handle.hierarchy_manager() {
            return Ok(Some(Placement {
                kind,
                status: handle.status().into(),
                parent: manager.parent_id().map(|p| p.to_string()),
                root: manager.root_execution_id().to_string(),
                depth: manager.depth(),
                ancestors: manager
                    .ancestors()
                    .iter()
                    .map(|a| a.to_string())
                    .collect(),
            }));
        }
    }
    for kind in KINDS {
        if let Some(row) = read_row(ctx, &kind, id).await? {
            return Ok(Some(Placement {
                kind: row.kind().clone(),
                status: row.status().cloned().unwrap_or(ExecutionStatus::Running),
                parent: row.parent().map(str::to_string),
                root: row.root().to_string(),
                depth: row.depth(),
                ancestors: row.ancestors(),
            }));
        }
    }
    Ok(None)
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
pub async fn hierarchy(ctx: &ApiContext, id: &str) -> ApiResult<ExecutionHierarchyView> {
    let placement = placement(ctx, id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(id))?;
    let parent = match &placement.parent {
        Some(parent_id) => Some(ExecutionRef {
            execution_id: parent_id.clone(),
            execution_type: execution_type(ctx, parent_id).await?,
        }),
        None => None,
    };
    let root = if placement.root == id {
        ExecutionRef {
            execution_id: id.to_string(),
            execution_type: placement.kind.clone(),
        }
    } else {
        ExecutionRef {
            execution_id: placement.root.clone(),
            execution_type: execution_type(ctx, &placement.root).await?,
        }
    };
    Ok(ExecutionHierarchyView {
        execution_id: id.to_string(),
        execution_type: placement.kind,
        status: placement.status,
        depth: placement.depth,
        parent,
        root,
        ancestors: placement.ancestors,
        children: children_of(ctx, id).await?,
    })
}

/// Direct children of one execution.
///
/// A live parent knows the children it spawned but not yet persisted, and a
/// persisted parent knows the children of earlier runs but has no live
/// manager, so the two sources are merged rather than one replacing the other.
async fn children_of(ctx: &ApiContext, id: &str) -> ApiResult<Vec<ExecutionRef>> {
    let mut children: Vec<ExecutionRef> = Vec::new();
    if let Some(handle) = ctx.execution_instance(id) {
        if let Some(manager) = handle.hierarchy_manager() {
            children.extend(manager.children().into_iter().map(|child| ExecutionRef {
                execution_id: child.child_id.to_string(),
                execution_type: child.child_type,
            }));
        }
    }
    for row in read_all_rows(ctx, &ExecutionIndexRow::children_filter(id)).await? {
        children.push(ExecutionRef {
            execution_id: row.id().to_string(),
            execution_type: row.kind().clone(),
        });
    }
    children.sort_by(|a, b| a.execution_id.cmp(&b.execution_id));
    children.dedup_by(|a, b| a.execution_id == b.execution_id);
    Ok(children)
}

/// Every execution in the tree rooted at `root_id`.
///
/// The materialised path makes this one prefix scan per record kind rather
/// than a query per node. Live executions have no row yet, so the live
/// managers are walked as well and the two sets are merged.
pub async fn subtree(ctx: &ApiContext, root_id: &str) -> ApiResult<ExecutionSubtree> {
    let root = placement(ctx, root_id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(root_id))?;

    let mut nodes: Vec<ExecutionSubtreeNode> =
        read_all_rows(ctx, &ExecutionIndexRow::subtree_filter(root_id))
            .await?
            .iter()
            .map(|row| ExecutionSubtreeNode {
                execution_id: row.id().to_string(),
                execution_type: row.kind().clone(),
                status: row.status().cloned(),
                depth: depth_within(row, root.depth),
                parent_execution_id: row.parent().map(str::to_string),
            })
            .collect();

    let mut seen: HashSet<String> = nodes.iter().map(|n| n.execution_id.clone()).collect();
    // A live root has no row for the prefix scan to find, so make sure the
    // node the caller asked about is always present exactly once.
    if seen.insert(root_id.to_string()) {
        nodes.push(ExecutionSubtreeNode {
            execution_id: root_id.to_string(),
            execution_type: root.kind.clone(),
            status: Some(root.status.clone()),
            depth: 0,
            parent_execution_id: None,
        });
    }
    let mut frontier: VecDeque<(String, u32)> = VecDeque::from([(root_id.to_string(), 0)]);
    while let Some((parent_id, parent_depth)) = frontier.pop_front() {
        for child in children_of(ctx, &parent_id).await? {
            if !seen.insert(child.execution_id.clone()) {
                continue;
            }
            frontier.push_back((child.execution_id.clone(), parent_depth + 1));
            nodes.push(ExecutionSubtreeNode {
                execution_id: child.execution_id,
                execution_type: child.execution_type,
                status: None,
                depth: parent_depth + 1,
                parent_execution_id: Some(parent_id.clone()),
            });
        }
    }

    nodes.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.execution_id.cmp(&b.execution_id)));
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

    async fn register_agent(ctx: &ApiContext, id: &str, hierarchy: Arc<wf_core::ExecutionHierarchyManager>) {
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
        Some(ExecutionHierarchy {
            workflow_id: manager.execution_id(),
            execution_id: manager.execution_id(),
            parent_execution_id: parent.as_ref().map(|p| p.parent_id.clone()),
            parent_execution_type: parent.as_ref().map(|p| p.parent_type.clone()),
            depth: manager.depth(),
            root_execution_id: Some(manager.root_execution_id()),
            root_execution_type: Some(manager.root_execution_type()),
            ancestors: if ancestors.is_empty() {
                None
            } else {
                Some(ancestors)
            },
            fork_path: manager.fork_path(),
        })
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
        assert!(view.children.is_empty());
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
        a.derive_child(Id::from("a1"), ExecutionType::AgentLoop, None)
            .unwrap();
        register_agent(&ctx, "root", root).await;
        register_agent(&ctx, "a", a).await;
        register_agent(&ctx, "b", b).await;

        let tree = subtree(&ctx, "root").await.unwrap();
        assert!(!tree.truncated);
        let order: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
        assert_eq!(order, vec!["root", "a", "b", "a1"]);
        let depths: Vec<u32> = tree.nodes.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1, 1, 2]);
        assert_eq!(tree.nodes[0].parent_execution_id, None);
        assert_eq!(tree.nodes[3].parent_execution_id.as_deref(), Some("a"));
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
            .save(&workflow_record("wf-root", persisted_hierarchy(&root_manager)))
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
        assert!(view.children.is_empty());
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

        for (id, mgr) in [
            ("r", &root),
            ("c", &child),
            ("g", &grandchild),
        ] {
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
    async fn branching_tree(ctx: &ApiContext) {
        let root = manager("r", ExecutionType::Workflow);
        let a = root
            .derive_child(Id::from("a"), ExecutionType::AgentLoop, None)
            .unwrap();
        let b = root
            .derive_child(Id::from("b"), ExecutionType::AgentLoop, None)
            .unwrap();
        let g = a.derive_child(Id::from("g"), ExecutionType::AgentLoop, None).unwrap();

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
            .save(&agent_record("stored-kid", persisted_hierarchy(&stored_child)))
            .await
            .unwrap();

        let view = hierarchy(&ctx, "live-root").await.unwrap();
        let ids: Vec<&str> = view
            .children
            .iter()
            .map(|c| c.execution_id.as_str())
            .collect();
        assert_eq!(ids, vec!["live-kid", "stored-kid"]);
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
}
