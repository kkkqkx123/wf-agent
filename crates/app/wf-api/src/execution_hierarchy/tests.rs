use super::*;
use crate::ApiContext;
use crate::ApiError;
use std::sync::Arc;
use wf_agent::entity::AgentLoopEntity;
use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::execution::{ExecutionHierarchy, ExecutionStatus, ExecutionType};
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
    let entity =
        Arc::new(AgentLoopEntity::new(Id::from(id.to_string())).with_hierarchy_manager(hierarchy));
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
        parent.as_ref().map(|p| p.parent_type),
        Some(manager.root_execution_type()),
        manager.fork_path(),
    ))
}

fn workflow_record(id: &str, hierarchy: Option<ExecutionHierarchy>) -> wf_types::WorkflowExecution {
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

        rerun_of: None,
        debug: None,
    }
}

fn agent_record(id: &str, hierarchy: Option<ExecutionHierarchy>) -> wf_types::AgentExecution {
    wf_types::AgentExecution {
        id: Id::from(id.to_string()),
        definition_id: Id::from("agent-1".to_string()),
        execution_type: wf_types::execution::ExecutionType::Agent,
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

        rerun_of: None,
    }
}

#[tokio::test]
async fn live_root_reports_no_parent() {
    let ctx = make_ctx();
    register_agent(
        &ctx,
        "live-root",
        manager("live-root", ExecutionType::Agent),
    )
    .await;

    let view = hierarchy(&ctx, "live-root").await.unwrap();
    assert_eq!(view.execution_id, "live-root");
    assert_eq!(view.execution_type, ExecutionType::Agent);
    assert_eq!(view.depth, 0);
    assert!(view.parent.is_none());
    assert_eq!(view.root.execution_id, "live-root");
    assert!(view.ancestors.is_empty());
}

#[tokio::test]
async fn live_child_reports_depth_root_and_parent() {
    let ctx = make_ctx();
    let root = manager("root", ExecutionType::Agent);
    let child = root
        .derive_child(Id::from("kid"), ExecutionType::Agent, None)
        .unwrap();
    register_agent(&ctx, "root", root).await;
    register_agent(&ctx, "kid", child).await;

    let view = hierarchy(&ctx, "kid").await.unwrap();
    assert_eq!(view.depth, 1);
    let parent = view.parent.expect("a derived child has a parent");
    assert_eq!(parent.execution_id, "root");
    assert_eq!(parent.execution_type, ExecutionType::Agent);
    assert_eq!(view.root.execution_id, "root");
    assert_eq!(view.ancestors, vec!["root".to_string()]);
}

#[tokio::test]
async fn subtree_walks_live_children_breadth_first() {
    let ctx = make_ctx();
    let root = manager("root", ExecutionType::Workflow);
    let a = root
        .derive_child(Id::from("a"), ExecutionType::Agent, None)
        .unwrap();
    let b = root
        .derive_child(Id::from("b"), ExecutionType::Workflow, None)
        .unwrap();
    let a1 = a
        .derive_child(Id::from("a1"), ExecutionType::Agent, None)
        .unwrap();
    register_agent(&ctx, "root", root).await;
    register_agent(&ctx, "a", a).await;
    register_agent(&ctx, "b", b).await;
    register_agent(&ctx, "a1", a1).await;

    let tree = subtree(&ctx, "root", usize::MAX, 0).await.unwrap();
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
        let tree = subtree(ctx, "r", usize::MAX, 0).await.unwrap();
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
        .derive_child(Id::from("agent-kid"), ExecutionType::Agent, None)
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
    assert_eq!(view.execution_type, ExecutionType::Agent);
    assert_eq!(view.status, ExecutionStatus::Failed);
    assert_eq!(view.depth, 1);
    assert_eq!(view.root.execution_id, "wf-root");
    assert_eq!(view.ancestors, vec!["wf-root".to_string()]);
    assert_eq!(view.parent.unwrap().execution_id, "wf-root");

    let tree = subtree(&ctx, "wf-root", usize::MAX, 0).await.unwrap();
    assert_eq!(tree.nodes.len(), 2);
    assert_eq!(tree.nodes[1].execution_type, ExecutionType::Agent);
}

/// A parent that last persisted before its children existed still reports
/// them, because the child records carry the link rather than the parent
/// carrying a snapshot of its children.
#[tokio::test]
async fn tree_survives_a_parent_that_predates_its_children() {
    let ctx = make_ctx();
    let root_manager = manager("wf-root", ExecutionType::Workflow);
    let child_manager = root_manager
        .derive_child(Id::from("agent-kid"), ExecutionType::Agent, None)
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

    let tree = subtree(&ctx, "wf-root", usize::MAX, 0).await.unwrap();
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

    let tree = subtree(&ctx, "solo", usize::MAX, 0).await.unwrap();
    assert_eq!(tree.nodes.len(), 1, "a root lists only itself");
    assert_eq!(tree.nodes[0].execution_id, "solo");
    assert!(tree.nodes[0].parent_execution_id.is_none());
}

#[tokio::test]
async fn ancestors_span_a_three_level_persisted_chain() {
    let ctx = make_ctx();
    let root = manager("r", ExecutionType::Workflow);
    let child = root
        .derive_child(Id::from("c"), ExecutionType::Agent, None)
        .unwrap();
    let grandchild = child
        .derive_child(Id::from("g"), ExecutionType::Agent, None)
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
            .derive_child(Id::from(format!("c{i}")), ExecutionType::Agent, None)
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
        .derive_child(Id::from("a"), ExecutionType::Agent, None)
        .unwrap();
    let b = root
        .derive_child(Id::from("b"), ExecutionType::Agent, None)
        .unwrap();
    let g = a
        .derive_child(Id::from("g"), ExecutionType::Agent, None)
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

    let tree = subtree(&ctx, "r", usize::MAX, 0).await.unwrap();
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

    let tree = subtree(&ctx, "a", usize::MAX, 0).await.unwrap();
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

    let tree = subtree(&ctx, "r", usize::MAX, 0).await.unwrap();
    let order: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
    assert!(!order.contains(&"rx"), "`/r/` must not match `/rx/`");
}

/// A live parent knows the child it spawned a moment ago; a persisted one
/// knows the children of earlier runs. Neither source is a superset, so
/// dropping either would hide real children.
#[tokio::test]
async fn direct_children_merge_live_and_persisted() {
    let ctx = make_ctx();
    let root = manager("live-root", ExecutionType::Agent);
    let live_child = root
        .derive_child(Id::from("live-kid"), ExecutionType::Agent, None)
        .unwrap();
    let stored_child = root
        .derive_child(Id::from("stored-kid"), ExecutionType::Agent, None)
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

    let tree = subtree(&ctx, "live-root", usize::MAX, 0).await.unwrap();
    let ids: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
    assert_eq!(ids, vec!["live-root", "live-kid", "stored-kid"]);
}

#[tokio::test]
async fn a_live_branch_with_no_records_yet_still_appears() {
    let ctx = make_ctx();
    let root = manager("run", ExecutionType::Agent);
    let child = root
        .derive_child(Id::from("agent-kid"), ExecutionType::Agent, None)
        .unwrap();
    register_agent(&ctx, "run", root).await;
    register_agent(&ctx, "agent-kid", child).await;

    let tree = subtree(&ctx, "run", usize::MAX, 0).await.unwrap();
    let order: Vec<&str> = tree.nodes.iter().map(|n| n.execution_id.as_str()).collect();
    assert_eq!(order, vec!["run", "agent-kid"]);
}

#[tokio::test]
async fn unknown_id_is_not_found() {
    let ctx = make_ctx();
    assert!(hierarchy(&ctx, "nope").await.is_err());
    assert!(subtree(&ctx, "nope", usize::MAX, 0).await.is_err());
}

/// Paging bounds the response without silently clipping the tree: a
/// short page reports how many nodes remain and the offset that
/// resumes them, and following the offsets walks the whole tree.
#[tokio::test]
async fn a_subtree_pages_without_silent_clipping() {
    let ctx = make_ctx();
    persisted_star(&ctx, 4).await;

    let first = subtree(&ctx, "r", 2, 0).await.unwrap();
    assert_eq!(first.nodes.len(), 2);
    assert!(first.truncated);
    assert_eq!(first.omitted, 3);
    assert_eq!(first.next_offset, Some(2));
    assert!(first.rejected.is_empty());

    let second = subtree(&ctx, "r", 2, 2).await.unwrap();
    assert_eq!(second.nodes.len(), 2);
    assert!(second.truncated);
    assert_eq!(second.omitted, 1);

    let last = subtree(&ctx, "r", 2, 4).await.unwrap();
    assert_eq!(last.nodes.len(), 1);
    assert!(!last.truncated);
    assert_eq!(last.omitted, 0);
    assert_eq!(last.next_offset, None);

    let whole = subtree(&ctx, "r", usize::MAX, 0).await.unwrap();
    assert_eq!(whole.nodes.len(), 5);
    assert!(!whole.truncated);
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

    let tree = subtree(&ctx, "r", usize::MAX, 0).await.unwrap();
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
        .derive_child(Id::from("kid"), ExecutionType::Agent, None)
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
        ExecutionType::Agent
    );

    let tree = subtree(&ctx, "r", usize::MAX, 0).await.unwrap();
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
