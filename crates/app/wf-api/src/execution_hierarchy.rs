//! Parent/child queries over the unified execution handle.
//!
//! A live execution answers through the hierarchy manager its entity owns; a
//! persisted workflow or agent record answers through the
//! `ExecutionHierarchy` stored on the record, so a terminated run keeps
//! reporting its place in the tree after the live registry drops it.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use serde::Serialize;
use wf_core::hierarchy::manager::ExecutionHierarchyManager;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::types::execution_instance::ExecutionKind;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::execution::{ExecutionHierarchy, ExecutionType};
use wf_types::{ExecutionStatus, Id};

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
    /// `None` when the parent still references the child but the child's own
    /// record is gone, so no status can be reported for it.
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
    /// Root first, then each level in child order.
    pub nodes: Vec<ExecutionSubtreeNode>,
}

/// Hierarchy position of one execution plus the engine that owns it.
struct Placement {
    kind: ExecutionType,
    status: ExecutionStatus,
    manager: Arc<ExecutionHierarchyManager>,
}

/// Node queued for subtree expansion; the placement travels with the node so
/// the root is resolved once instead of twice.
struct Pending {
    node: ExecutionSubtreeNode,
    placement: Placement,
}

/// Resolve one execution's placement from the live registries, then from the
/// persisted records of both engines. `None` when the id is unknown to every
/// layer.
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
                manager,
            }));
        }
    }
    if let Some(record) = ctx.storage.workflow_execution.load(id).await? {
        return Ok(Some(Placement {
            kind: ExecutionType::Workflow,
            status: record.status,
            manager: record_manager(id, ExecutionType::Workflow, record.hierarchy.as_ref()),
        }));
    }
    Ok(ctx
        .storage
        .agent_execution
        .load(id)
        .await?
        .map(|record| Placement {
            kind: ExecutionType::AgentLoop,
            status: record.status,
            manager: record_manager(id, ExecutionType::AgentLoop, record.hierarchy.as_ref()),
        }))
}

/// Rebuild the hierarchy manager of a persisted record. A record without a
/// hierarchy was never linked to a parent, so it is a root of its own.
fn record_manager(
    id: &str,
    kind: ExecutionType,
    hierarchy: Option<&ExecutionHierarchy>,
) -> Arc<ExecutionHierarchyManager> {
    let execution_id = Id::from(id.to_string());
    match hierarchy {
        Some(hierarchy) => {
            ExecutionHierarchyManager::restore(execution_id, kind.clone(), hierarchy, kind.clone())
        }
        None => Arc::new(ExecutionHierarchyManager::new(execution_id, kind)),
    }
}

/// Child references in id order. The manager keeps children in a hash map,
/// so sorting is what makes the output stable.
fn children_of(manager: &ExecutionHierarchyManager) -> Vec<ExecutionRef> {
    let mut children: Vec<ExecutionRef> = manager
        .children()
        .into_iter()
        .map(|child| ExecutionRef {
            execution_id: child.child_id.to_string(),
            execution_type: child.child_type,
        })
        .collect();
    children.sort_by(|a, b| a.execution_id.cmp(&b.execution_id));
    children
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
    let manager = &placement.manager;
    Ok(ExecutionHierarchyView {
        execution_id: id.to_string(),
        execution_type: placement.kind,
        status: placement.status,
        depth: manager.depth(),
        parent: manager.parent().map(|parent| ExecutionRef {
            execution_id: parent.parent_id.to_string(),
            execution_type: parent.parent_type,
        }),
        root: ExecutionRef {
            execution_id: manager.root_execution_id().to_string(),
            execution_type: manager.root_execution_type(),
        },
        ancestors: manager
            .ancestors()
            .iter()
            .map(|ancestor| ancestor.to_string())
            .collect(),
        children: children_of(manager),
    })
}

/// Root-to-parent id chain of one execution, oldest first.
pub async fn ancestors(ctx: &ApiContext, id: &str) -> ApiResult<Vec<String>> {
    let placement = placement(ctx, id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(id))?;
    Ok(placement
        .manager
        .ancestors()
        .iter()
        .map(|ancestor| ancestor.to_string())
        .collect())
}

/// Every execution rooted at `root_id`, breadth-first.
pub async fn subtree(ctx: &ApiContext, root_id: &str) -> ApiResult<ExecutionSubtree> {
    let root = placement(ctx, root_id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(root_id))?;
    let root_node = ExecutionSubtreeNode {
        execution_id: root_id.to_string(),
        execution_type: root.kind.clone(),
        status: Some(root.status.clone()),
        depth: 0,
        parent_execution_id: None,
    };
    let mut nodes = vec![root_node.clone()];
    let mut visited = HashSet::from([root_id.to_string()]);
    let mut queue: VecDeque<Pending> = VecDeque::from([Pending {
        node: root_node,
        placement: root,
    }]);
    let mut truncated = false;

    while let Some(Pending {
        node,
        placement: parent,
    }) = queue.pop_front()
    {
        for child in children_of(&parent.manager) {
            if !visited.insert(child.execution_id.clone()) {
                continue;
            }
            if nodes.len() == MAX_SUBTREE_NODES {
                truncated = true;
                break;
            }
            let child_placement = placement(ctx, &child.execution_id).await?;
            let child_node = ExecutionSubtreeNode {
                execution_id: child.execution_id,
                execution_type: child.execution_type,
                status: child_placement.as_ref().map(|p| p.status.clone()),
                depth: node.depth + 1,
                parent_execution_id: Some(node.execution_id.clone()),
            };
            nodes.push(child_node.clone());
            // A child reference outliving the child's own record leaves a leaf
            // with no placement to descend into.
            if let Some(child_placement) = child_placement {
                queue.push_back(Pending {
                    node: child_node,
                    placement: child_placement,
                });
            }
        }
        if truncated {
            break;
        }
    }

    Ok(ExecutionSubtree {
        root_execution_id: root_id.to_string(),
        truncated,
        nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_agent::entity::AgentLoopEntity;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;
    use wf_types::execution::ChildExecutionReference;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    fn manager(id: &str, kind: ExecutionType) -> Arc<ExecutionHierarchyManager> {
        Arc::new(ExecutionHierarchyManager::new(
            Id::from(id.to_string()),
            kind,
        ))
    }

    fn child(parent: &Arc<ExecutionHierarchyManager>, id: &str, kind: ExecutionType) {
        parent.register_child_ref(ChildExecutionReference {
            child_type: kind,
            child_id: Id::from(id.to_string()),
            created_at: 1,
            fork_path: None,
        });
    }

    async fn register_agent(ctx: &ApiContext, id: &str, hierarchy: Arc<ExecutionHierarchyManager>) {
        let entity = Arc::new(
            AgentLoopEntity::new(Id::from(id.to_string())).with_hierarchy_manager(hierarchy),
        );
        let _ = ctx.agent_loops.register(entity);
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
        assert_eq!(view.root.execution_type, ExecutionType::AgentLoop);
        assert!(view.ancestors.is_empty());
    }

    #[tokio::test]
    async fn live_child_reports_depth_and_root() {
        let ctx = make_ctx();
        let root = manager("root", ExecutionType::Workflow);
        let child = root
            .derive_child(Id::from("kid"), ExecutionType::AgentLoop, None)
            .unwrap();
        register_agent(&ctx, "kid", child).await;

        let view = hierarchy(&ctx, "kid").await.unwrap();
        assert_eq!(view.depth, 1);
        let parent = view.parent.expect("a derived child has a parent");
        assert_eq!(parent.execution_id, "root");
        assert_eq!(parent.execution_type, ExecutionType::Workflow);
        assert_eq!(view.root.execution_id, "root");
        assert_eq!(view.ancestors, vec!["root".to_string()]);
        assert_eq!(ancestors(&ctx, "kid").await.unwrap(), vec!["root"]);
    }

    #[tokio::test]
    async fn subtree_walks_live_children_breadth_first() {
        let ctx = make_ctx();
        let root = manager("root", ExecutionType::Workflow);
        let a = root
            .derive_child(Id::from("a"), ExecutionType::AgentLoop, None)
            .unwrap();
        root.derive_child(Id::from("b"), ExecutionType::Workflow, None)
            .unwrap();
        a.derive_child(Id::from("a1"), ExecutionType::AgentLoop, None)
            .unwrap();
        register_agent(&ctx, "root", root.clone()).await;
        register_agent(&ctx, "a", a).await;

        let tree = subtree(&ctx, "root").await.unwrap();
        assert!(!tree.truncated);
        let order: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
        assert_eq!(order, vec!["root", "a", "b", "a1"]);
        let depths: Vec<u32> = tree.nodes.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1, 1, 2]);
        assert_eq!(tree.nodes[0].parent_execution_id, None);
        assert_eq!(tree.nodes[3].parent_execution_id.as_deref(), Some("a"));
        // `b` was never registered, so the parent still references it but no
        // status can be reported for it.
        assert_eq!(tree.nodes[2].status, None);
    }

    #[tokio::test]
    async fn subtree_visits_each_node_once() {
        let ctx = make_ctx();
        let root = manager("root", ExecutionType::Workflow);
        child(&root, "shared", ExecutionType::AgentLoop);
        child(&root, "shared", ExecutionType::Workflow);
        register_agent(&ctx, "root", root).await;

        let tree = subtree(&ctx, "root").await.unwrap();
        assert_eq!(tree.nodes.len(), 2);
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

        let tree = subtree(&ctx, "wf-root").await.unwrap();
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[1].execution_type, ExecutionType::AgentLoop);
    }

    /// The persisted form of a manager: the same fields the engines write onto
    /// their execution records.
    fn persisted_hierarchy(manager: &Arc<ExecutionHierarchyManager>) -> Option<ExecutionHierarchy> {
        let metadata = manager.to_metadata();
        let (parent_id, parent_type) = match metadata.parent {
            Some(parent) => (Some(parent.parent_id), Some(parent.parent_type)),
            None => (None, None),
        };
        Some(ExecutionHierarchy {
            workflow_id: manager.execution_id(),
            execution_id: manager.execution_id(),
            parent_execution_id: parent_id,
            parent_execution_type: parent_type,
            depth: metadata.depth,
            root_execution_id: Some(metadata.root_execution_id),
            root_execution_type: Some(metadata.root_execution_type),
            ancestors: metadata.ancestors,
            children: Some(metadata.children),
        })
    }

    #[tokio::test]
    async fn unknown_id_is_not_found() {
        let ctx = make_ctx();
        assert!(hierarchy(&ctx, "nope").await.is_err());
        assert!(ancestors(&ctx, "nope").await.is_err());
        assert!(subtree(&ctx, "nope").await.is_err());
    }
}
