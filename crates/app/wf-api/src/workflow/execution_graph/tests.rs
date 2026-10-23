use super::*;
use crate::ApiContext;
use std::sync::Arc;
use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::workflow::edge::EdgeType;
use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

fn node(id: &str, node_type: &str) -> WorkflowNode {
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: node_type.to_string(),
        inner: serde_json::json!({}),
    }
}

fn edge(source: &str, target: &str, condition: Option<&str>) -> WorkflowEdge {
    WorkflowEdge {
        id: format!("{}-{}", source, target),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        r#type: EdgeType::Default,
        condition: condition.map(String::from),
        label: None,
        description: None,
        error_route: None,
    }
}

fn graph() -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes: vec![
            node("start", "START"),
            node("route", "ROUTE"),
            node("a", "VARIABLE"),
            node("b", "VARIABLE"),
            node("end", "END"),
        ],
        edges: vec![
            edge("start", "route", None),
            edge("route", "a", Some("${input.x} > 0")),
            edge("route", "b", Some("${input.x} <= 0")),
            edge("a", "end", None),
            edge("b", "end", None),
        ],
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    }
}

#[test]
fn enumerates_all_paths() {
    let paths = enumerate_paths(&graph());
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().any(|p| p.nodes.contains(&"a".to_string())));
    assert!(paths.iter().any(|p| p.nodes.contains(&"b".to_string())));
    // Critical path length is identical here (start->route->a->end).
    assert_eq!(paths[0].length, 4);
}

#[test]
fn decision_points_detect_taken_alternative() {
    let graph = graph();
    let executed: Vec<String> = vec!["start", "route", "b", "end"]
        .into_iter()
        .map(String::from)
        .collect();
    let points = analyze_decision_points(&graph, &executed);
    assert_eq!(points.len(), 1);
    let point = &points[0];
    assert_eq!(point.node_id, "route");
    assert_eq!(point.alternative_count, 2);
    assert_eq!(point.taken_edge.as_deref(), Some("route-b"));
    assert_eq!(point.untaken_conditions, vec!["${input.x} > 0".to_string()]);
}

#[tokio::test]
async fn analyze_degrades_gracefully() {
    let ctx = Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ));
    // No live entity and no persisted record: empty analysis, no error.
    let analysis = analyze(&ctx, "missing-exec").await.unwrap();
    assert!(analysis.paths.is_empty());
    assert!(analysis.executed_nodes.is_empty());
}

async fn persist(ctx: &ApiContext, execution_id: &str, workflow_id: &str) {
    let record = wf_types::WorkflowExecution {
        id: execution_id.into(),
        workflow_id: workflow_id.into(),
        workflow_version: None,
        status: wf_types::ExecutionStatus::Completed,
        current_node_id: None,
        graph: Some(graph()),
        variables: None,
        input: None,
        output: None,
        node_results: Some(vec![
            wf_types::workflow_execution::NodeExecutionResult {
                node_id: "start".into(),
                status: "completed".into(),
                input: None,
                output: None,
                error: None,
                started_at: Some(1000),
                completed_at: Some(1100),
            },
            wf_types::workflow_execution::NodeExecutionResult {
                node_id: "route".into(),
                status: "completed".into(),
                input: None,
                output: None,
                error: None,
                started_at: Some(1100),
                completed_at: Some(1200),
            },
            wf_types::workflow_execution::NodeExecutionResult {
                node_id: "b".into(),
                status: "completed".into(),
                input: None,
                output: None,
                error: None,
                started_at: Some(1200),
                completed_at: Some(1500),
            },
            wf_types::workflow_execution::NodeExecutionResult {
                node_id: "b".into(),
                status: "completed".into(),
                input: None,
                output: None,
                error: None,
                started_at: Some(1500),
                completed_at: Some(3500),
            },
            wf_types::workflow_execution::NodeExecutionResult {
                node_id: "end".into(),
                status: "completed".into(),
                input: None,
                output: None,
                error: None,
                started_at: Some(3500),
                completed_at: Some(3600),
            },
        ]),
        errors: None,
        error: None,
        started_at: 1000,
        completed_at: Some(3600),
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,
    };
    ctx.storage.workflow_execution.save(&record).await.unwrap();
}

#[tokio::test]
async fn record_and_clear_execution_graph() {
    let ctx = Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ));
    persist(&ctx, "exec-g", "wf-g").await;

    let recorded = record_execution_graph(&ctx, "exec-g").await.unwrap();
    assert_eq!(recorded.nodes.len(), 5);

    let record = ctx
        .storage
        .workflow_execution
        .load("exec-g")
        .await
        .unwrap()
        .unwrap();
    assert!(record.graph.is_some());

    clear_execution_data(&ctx, "exec-g").await.unwrap();
    let record = ctx
        .storage
        .workflow_execution
        .load("exec-g")
        .await
        .unwrap()
        .unwrap();
    assert!(record.graph.is_none());
}

#[tokio::test]
async fn slow_nodes_and_efficiency() {
    let ctx = Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ));
    persist(&ctx, "exec-eff", "wf-eff").await;

    let slow = get_slow_nodes(&ctx, "exec-eff", 0.8).await.unwrap();
    assert!(
        !slow.is_empty(),
        "b (2000ms) should exceed the 80th percentile"
    );
    assert!(slow.iter().any(|s| s.node_id == "b"));
    assert!(slow.iter().all(|s| s.duration_ms > 0));

    let efficiency = analyze_efficiency(&ctx, "exec-eff").await.unwrap();
    // 5 successful executions across 4 distinct nodes; shortest path = 4.
    assert!(efficiency.optimal_steps >= 1);
    assert_eq!(efficiency.executed_steps, 5);
    assert!(efficiency.efficiency_ratio >= 1.0);
    assert_eq!(efficiency.retry_count, 0, "persisted results all completed");
}

#[tokio::test]
async fn alternative_paths_and_probability() {
    let ctx = Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ));
    persist(&ctx, "exec-prob", "wf-prob").await;

    let alternatives = get_alternative_paths(&ctx, "exec-prob").await.unwrap();
    assert_eq!(alternatives.len(), 1, "the untaken 'route -> a' branch");
    assert_eq!(alternatives[0].node_id, "a");

    let probability = get_path_probability_analysis(&ctx, "exec-prob")
        .await
        .unwrap();
    assert_eq!(probability.paths.len(), 2);
    for path in &probability.paths {
        assert!(
            (path.probability - 0.5).abs() < 0.001,
            "uniform branch split"
        );
    }
    assert!(probability.most_likely_path.is_some());
    assert!(probability.path_diversity >= 0.0);
    assert!(probability.paths.iter().any(|p| p.is_taken));
}
