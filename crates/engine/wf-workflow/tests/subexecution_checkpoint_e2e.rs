//! End-to-end tests for sub-execution checkpoint snapshots.
//!
//! Covers the two cross-cutting chains the unit tests cannot: a forked run
//! whose parent snapshot carries live branch aggregation, and a subgraph run
//! whose parent/child snapshots preserve the hierarchy linkage across the
//! restore boundary.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;
use wf_checkpoint::coordinator::CheckpointCoordinator;
use wf_checkpoint::state::{CheckpointStateManager, WorkflowCheckpointStateManager};
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_storage::backend::StorageBackend;
use wf_tools::registry::ToolRegistry;
use wf_types::node::StaticNodeType;
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::coordinator::WorkflowExecutionParams;
use wf_workflow::handler::NodeHandler;
use wf_workflow::{HandlerRegistry, NodeCheckpointStrategy, WorkflowLifecycleCoordinator};

struct RecordingScript {
    recorded: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl NodeHandler for RecordingScript {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        _ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.recorded.lock().unwrap().push("ran".to_string());
        Ok(NodeExecutionResult::simple(serde_json::json!({})))
    }
}

fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: node_type.to_string(),
        inner,
    }
}

fn edge(source: &str, target: &str) -> WorkflowEdge {
    WorkflowEdge {
        id: format!("{}-{}", source, target),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        r#type: EdgeType::Default,
        condition: None,
        label: None,
        description: None,
        error_route: None,
    }
}

fn graph(
    nodes: Vec<WorkflowNode>,
    edges: Vec<WorkflowEdge>,
    start: &str,
    ends: Vec<&str>,
) -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id: Some(start.to_string()),
        end_node_ids: ends.into_iter().map(String::from).collect(),
        error_default: None,
    }
}

fn checkpoint_options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(true),
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

fn recording_handlers(
    recorded: Arc<std::sync::Mutex<Vec<String>>>,
) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(RecordingScript { recorded }));
    reg.into_arc()
}

fn lifecycle(store: Arc<StorageBackend>) -> WorkflowLifecycleCoordinator {
    WorkflowLifecycleCoordinator::with_store(None, store)
        .with_checkpoint_strategy(NodeCheckpointStrategy::every_node())
        .with_signal_bus(Arc::new(wf_core::internal_signal::InternalSignalBus::new()))
}

async fn restore_latest_snapshot(
    store: &Arc<StorageBackend>,
    execution_id: &str,
) -> wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot {
    let manager = WorkflowCheckpointStateManager::new(store.clone());
    let meta = manager
        .get_latest(execution_id)
        .await
        .expect("latest lookup succeeds")
        .expect("execution wrote at least one checkpoint");
    let coordinator =
        WorkflowCheckpointCoordinator::new(WorkflowCheckpointStateManager::new(store.clone()));
    coordinator
        .restore(&meta.id)
        .await
        .expect("restore succeeds")
        .snapshot
}

fn fork_join_graph() -> WorkflowGraphStructure {
    graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "fork",
                "FORK",
                serde_json::json!({
                    "fork_paths": [
                        {"path_id": "p1", "child_node_id": "a"},
                        {"path_id": "p2", "child_node_id": "b"}
                    ]
                }),
            ),
            node(
                "a",
                "SCRIPT",
                serde_json::json!({"script_name": "sA", "risk": "medium"}),
            ),
            node(
                "b",
                "SCRIPT",
                serde_json::json!({"script_name": "sB", "risk": "medium"}),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1", "p2"], "join_strategy": "wait_for_all"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "fork"),
            edge("fork", "a"),
            edge("fork", "b"),
            edge("a", "join"),
            edge("b", "join"),
            edge("join", "end"),
        ],
        "start",
        vec!["end"],
    )
}

#[tokio::test]
async fn fork_completion_snapshot_carries_live_aggregation() {
    let store = Arc::new(StorageBackend::new_memory());
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let execution_id = "exec-fork-snapshot";
    lifecycle(store.clone())
        .execute_workflow(WorkflowExecutionParams {
            execution_id: execution_id.into(),
            workflow_id: "wf-fork-snapshot".into(),
            graph: fork_join_graph(),
            options: checkpoint_options(),
            handlers: recording_handlers(recorded.clone()),
            tool_registry: Arc::new(ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        })
        .await
        .expect("fork workflow completes");
    assert_eq!(recorded.lock().unwrap().len(), 2, "both branches ran");

    let snapshot = restore_latest_snapshot(&store, execution_id).await;
    let aggregation = snapshot
        .fork_join_aggregation_state
        .expect("parent snapshot carries aggregation");
    assert_eq!(
        aggregation["pathStatuses"]["p1"],
        serde_json::json!("COMPLETED")
    );
    assert_eq!(
        aggregation["pathStatuses"]["p2"],
        serde_json::json!("COMPLETED")
    );
    assert_eq!(
        aggregation["isAggregationComplete"],
        serde_json::json!(true)
    );
    let branch_children = aggregation
        .get("branchChildren")
        .and_then(|v| v.as_object())
        .expect("aggregation maps paths to branch executions");
    assert_eq!(branch_children.len(), 2);

    // The branch map and the hierarchy child references describe the same
    // linkage: every mapped branch id is a registered fork child.
    let hierarchy = snapshot
        .hierarchy
        .expect("parent snapshot carries hierarchy");
    let children = hierarchy.children.expect("parent links both branches");
    assert_eq!(children.len(), 2);
    for child in &children {
        assert_eq!(child.fork_node_id(), Some("fork"));
        assert!(child.branch_path_id().is_some());
    }
    let mut mapped: Vec<&str> = branch_children
        .values()
        .filter_map(|v| v.as_str())
        .collect();
    mapped.sort();
    let mut linked: Vec<&str> = children.iter().map(|c| c.child_id.as_str()).collect();
    linked.sort();
    assert_eq!(mapped, linked);
}

#[tokio::test]
async fn subgraph_completion_snapshot_preserves_child_linkage() {
    let store = Arc::new(StorageBackend::new_memory());
    let child_graph_id = format!("child-{}", wf_types::Id::new());
    wf_workflow::register_graph(
        &child_graph_id,
        graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "v1",
                    "VARIABLE",
                    serde_json::json!({"variable_name": "result_val", "expression": "21 * 2"}),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![edge("start", "v1"), edge("v1", "end")],
            "start",
            vec!["end"],
        ),
    );
    let parent_graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "sub",
                "SUBGRAPH",
                serde_json::json!({
                    "subgraph_id": child_graph_id,
                    "variable_outputs": [{"internal_name": "result_val", "target_path": "final_val"}]
                }),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "sub"), edge("sub", "end")],
        "start",
        vec!["end"],
    );
    let execution_id = "exec-subgraph-snapshot";
    let output = lifecycle(store.clone())
        .execute_workflow(WorkflowExecutionParams {
            execution_id: execution_id.into(),
            workflow_id: "wf-subgraph-snapshot".into(),
            graph: parent_graph,
            options: checkpoint_options(),
            handlers: recording_handlers(Arc::new(std::sync::Mutex::new(Vec::new()))),
            tool_registry: Arc::new(ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        })
        .await
        .expect("subgraph workflow completes");
    assert_eq!(output.execution_id.as_str(), execution_id);

    // The parent snapshot links exactly one child; the child's own snapshot
    // carries the matching parent, depth, root and ancestor chain.
    let parent_snapshot = restore_latest_snapshot(&store, execution_id).await;
    let parent_hierarchy = parent_snapshot
        .hierarchy
        .expect("parent snapshot carries hierarchy");
    let parent_children = parent_hierarchy.children.expect("parent links the child");
    assert_eq!(parent_children.len(), 1);
    assert_eq!(
        parent_children[0].child_type,
        wf_types::execution::ExecutionType::Workflow
    );
    let child_id = parent_children[0].child_id.to_string();

    let child_snapshot = restore_latest_snapshot(&store, &child_id).await;
    let child_hierarchy = child_snapshot
        .hierarchy
        .expect("child snapshot carries hierarchy");
    assert_eq!(
        child_hierarchy.parent_execution_id.as_deref(),
        Some(execution_id)
    );
    assert_eq!(child_hierarchy.depth, 1);
    assert_eq!(
        child_hierarchy.root_execution_id.as_deref(),
        Some(execution_id)
    );
    assert_eq!(
        child_hierarchy.ancestors,
        Some(vec![execution_id.to_string()])
    );
}
