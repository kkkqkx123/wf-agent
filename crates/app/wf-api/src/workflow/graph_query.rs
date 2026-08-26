//! Workflow graph structure query API.
//!
//! Thin wrappers: the graph conversion lives in `workflow_execution`
//! (`definition_to_graph` / `resolve_graph`) and the graph algorithms in
//! `wf-workflow::analysis`. This module only exposes them as query functions
//! over a stored workflow id.

use std::collections::BTreeMap;

use serde::Serialize;

use wf_core::registry::Registry;
use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Node view of a workflow graph.
#[derive(Debug, Clone, Serialize)]
pub struct GraphNodeView {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub node_type: String,
}

/// Edge view of a workflow graph.
#[derive(Debug, Clone, Serialize)]
pub struct GraphEdgeView {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub edge_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// Aggregate summary of a workflow graph.
#[derive(Debug, Clone, Serialize)]
pub struct GraphSummary {
    pub workflow_id: String,
    pub node_count: usize,
    pub edge_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_node_id: Option<String>,
    pub end_node_ids: Vec<String>,
    pub node_counts_by_type: BTreeMap<String, usize>,
}

/// Neighbors of one graph node: its predecessors and successors.
#[derive(Debug, Clone, Serialize)]
pub struct GraphNeighborsView {
    pub node_id: String,
    pub predecessors: Vec<String>,
    pub successors: Vec<String>,
}

/// Resolve the executable graph of a stored workflow (validated).
pub async fn get_graph(ctx: &ApiContext, workflow_id: &str) -> ApiResult<WorkflowGraphStructure> {
    crate::workflow::workflow_execution::resolve_graph(ctx, workflow_id).await
}

/// Aggregate summary of a workflow graph.
pub async fn graph_summary(ctx: &ApiContext, workflow_id: &str) -> ApiResult<GraphSummary> {
    let graph = get_graph(ctx, workflow_id).await?;
    let mut node_counts_by_type = BTreeMap::new();
    for node in &graph.nodes {
        *node_counts_by_type
            .entry(node.node_type.clone())
            .or_insert(0) += 1;
    }
    Ok(GraphSummary {
        workflow_id: workflow_id.to_string(),
        node_count: graph.nodes.len(),
        edge_count: graph.edges.len(),
        start_node_id: graph.start_node_id,
        end_node_ids: graph.end_node_ids,
        node_counts_by_type,
    })
}

/// All nodes of a workflow graph.
pub async fn graph_nodes(ctx: &ApiContext, workflow_id: &str) -> ApiResult<Vec<GraphNodeView>> {
    Ok(get_graph(ctx, workflow_id)
        .await?
        .nodes
        .into_iter()
        .map(graph_node_view)
        .collect())
}

/// Nodes of a workflow graph filtered by node type.
pub async fn graph_nodes_by_type(
    ctx: &ApiContext,
    workflow_id: &str,
    node_type: &str,
) -> ApiResult<Vec<GraphNodeView>> {
    Ok(get_graph(ctx, workflow_id)
        .await?
        .nodes
        .into_iter()
        .filter(|n| n.node_type == node_type)
        .map(graph_node_view)
        .collect())
}

/// All edges of a workflow graph.
pub async fn graph_edges(ctx: &ApiContext, workflow_id: &str) -> ApiResult<Vec<GraphEdgeView>> {
    Ok(get_graph(ctx, workflow_id)
        .await?
        .edges
        .into_iter()
        .map(graph_edge_view)
        .collect())
}

/// Predecessors and successors of a graph node.
pub async fn graph_node_neighbors(
    ctx: &ApiContext,
    workflow_id: &str,
    node_id: &str,
) -> ApiResult<GraphNeighborsView> {
    let graph = get_graph(ctx, workflow_id).await?;
    Ok(GraphNeighborsView {
        node_id: node_id.to_string(),
        predecessors: graph
            .edges
            .iter()
            .filter(|e| e.target_node_id == node_id)
            .map(|e| e.source_node_id.clone())
            .collect(),
        successors: graph
            .edges
            .iter()
            .filter(|e| e.source_node_id == node_id)
            .map(|e| e.target_node_id.clone())
            .collect(),
    })
}

/// Full structural analysis of a workflow graph (cycles / topo sort /
/// reachability / node-type distribution). Passthrough to
/// `wf-workflow::analysis::analyze_graph`.
pub async fn graph_analysis(
    ctx: &ApiContext,
    workflow_id: &str,
) -> ApiResult<wf_workflow::analysis::GraphAnalysis> {
    let graph = get_graph(ctx, workflow_id).await?;
    Ok(wf_workflow::analysis::analyze_graph(&graph))
}

/// Detect structural cycles in a workflow graph.
pub async fn graph_detect_cycles(
    ctx: &ApiContext,
    workflow_id: &str,
) -> ApiResult<wf_workflow::analysis::CycleDetectionResult> {
    let graph = get_graph(ctx, workflow_id).await?;
    Ok(wf_workflow::analysis::detect_cycles(&graph))
}

/// Topological sort of a workflow graph.
pub async fn graph_topological_sort(
    ctx: &ApiContext,
    workflow_id: &str,
) -> ApiResult<wf_workflow::analysis::TopologicalSortResult> {
    let graph = get_graph(ctx, workflow_id).await?;
    Ok(wf_workflow::analysis::topological_sort(&graph))
}

/// Reachability analysis of a workflow graph.
pub async fn graph_reachability(
    ctx: &ApiContext,
    workflow_id: &str,
) -> ApiResult<wf_workflow::analysis::ReachabilityResult> {
    let graph = get_graph(ctx, workflow_id).await?;
    Ok(wf_workflow::analysis::analyze_reachability(&graph))
}

/// Ids of the workflows currently registered in the execution index.
pub async fn list_graph_workflows(ctx: &ApiContext) -> ApiResult<Vec<String>> {
    Ok(ctx.registries.workflows.list())
}

fn graph_node_view(node: WorkflowNode) -> GraphNodeView {
    GraphNodeView {
        id: node.id,
        name: node.name,
        node_type: node.node_type,
    }
}

fn graph_edge_view(edge: WorkflowEdge) -> GraphEdgeView {
    GraphEdgeView {
        id: edge.id,
        source_node_id: edge.source_node_id,
        target_node_id: edge.target_node_id,
        edge_type: format!("{:?}", edge.r#type),
        condition: edge.condition,
    }
}

// ── execution graph queries ─────────────────────────────────────────────

/// Resolve the graph of an executed workflow: the persisted execution's own
/// graph when present, otherwise its workflow definition converted to a
/// graph. Empty graph when the execution cannot be resolved.
pub async fn get_execution_graph(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<WorkflowGraphStructure> {
    Ok(
        crate::workflow::execution_graph::resolve_graph(ctx, execution_id)
            .await?
            .unwrap_or_else(|| WorkflowGraphStructure {
                nodes: Vec::new(),
                edges: Vec::new(),
                adjacency_list: Default::default(),
                reverse_adjacency_list: Default::default(),
                start_node_id: None,
                end_node_ids: Vec::new(),
            }),
    )
}

/// Nodes of an execution's graph.
pub async fn execution_graph_nodes(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<GraphNodeView>> {
    Ok(get_execution_graph(ctx, execution_id)
        .await?
        .nodes
        .into_iter()
        .map(graph_node_view)
        .collect())
}

/// Edges of an execution's graph.
pub async fn execution_graph_edges(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<GraphEdgeView>> {
    Ok(get_execution_graph(ctx, execution_id)
        .await?
        .edges
        .into_iter()
        .map(graph_edge_view)
        .collect())
}

/// Neighbors of a node in an execution's graph.
pub async fn execution_graph_node_neighbors(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<GraphNeighborsView> {
    let graph = get_execution_graph(ctx, execution_id).await?;
    Ok(GraphNeighborsView {
        node_id: node_id.to_string(),
        predecessors: graph
            .edges
            .iter()
            .filter(|e| e.target_node_id == node_id)
            .map(|e| e.source_node_id.clone())
            .collect(),
        successors: graph
            .edges
            .iter()
            .filter(|e| e.source_node_id == node_id)
            .map(|e| e.target_node_id.clone())
            .collect(),
    })
}

/// Path statistics of an execution: the resolved execution paths and the
/// node-type distribution.
pub async fn get_execution_path_statistics(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ExecutionPathStatsView>> {
    let graph = get_execution_graph(ctx, execution_id).await?;
    let paths = crate::workflow::execution_graph::enumerate_paths(&graph);
    Ok(paths.into_iter().map(execution_path_stats).collect())
}

/// Digest of one resolved execution path.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPathStatsView {
    pub node_count: usize,
    pub edge_count: usize,
    pub nodes: Vec<String>,
}

fn execution_path_stats(
    path: crate::workflow::execution_graph::ExecutionPath,
) -> ExecutionPathStatsView {
    let edge_count = path.nodes.len().saturating_sub(1);
    ExecutionPathStatsView {
        node_count: path.nodes.len(),
        edge_count,
        nodes: path.nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    use crate::workflow::save_workflow;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    fn make_workflow(id: &str) -> wf_types::WorkflowDefinition {
        wf_types::WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {id}"),
            description: None,
            r#type: None,
            version: None,
            nodes: vec![
                wf_types::node::BaseStaticNode {
                    id: "start".into(),
                    node_type: wf_types::node::StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                wf_types::node::BaseStaticNode {
                    id: "llm-1".into(),
                    node_type: wf_types::node::StaticNodeType::Llm,
                    name: Some("llm".into()),
                    description: None,
                    config: Some(serde_json::json!({ "profile_id": "default" })),
                    execution_config: None,
                },
                wf_types::node::BaseStaticNode {
                    id: "end".into(),
                    node_type: wf_types::node::StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![
                wf_types::workflow::Edge {
                    id: "e1".into(),
                    source_node_id: "start".into(),
                    target_node_id: "llm-1".into(),
                    r#type: wf_types::workflow::EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
                wf_types::workflow::Edge {
                    id: "e2".into(),
                    source_node_id: "llm-1".into(),
                    target_node_id: "end".into(),
                    r#type: wf_types::workflow::EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
            ],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            hooks: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    #[tokio::test]
    async fn graph_query_api_smoke() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("gq-1")).await.unwrap();

        let summary = graph_summary(&ctx, "gq-1").await.unwrap();
        assert_eq!(summary.node_count, 3);
        assert_eq!(summary.edge_count, 2);
        assert_eq!(summary.start_node_id.as_deref(), Some("start"));
        assert_eq!(summary.node_counts_by_type.get("LLM"), Some(&1));

        let nodes = graph_nodes(&ctx, "gq-1").await.unwrap();
        assert_eq!(nodes.len(), 3);

        let llm = graph_nodes_by_type(&ctx, "gq-1", "LLM").await.unwrap();
        assert_eq!(llm.len(), 1);

        let edges = graph_edges(&ctx, "gq-1").await.unwrap();
        assert_eq!(edges.len(), 2);

        let neighbors = graph_node_neighbors(&ctx, "gq-1", "llm-1").await.unwrap();
        assert_eq!(neighbors.predecessors, vec!["start".to_string()]);
        assert_eq!(neighbors.successors, vec!["end".to_string()]);
    }

    #[tokio::test]
    async fn graph_analysis_passthrough_and_registry_list() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("gq-2")).await.unwrap();

        let analysis = graph_analysis(&ctx, "gq-2").await.unwrap();
        assert!(!analysis.cycle_detection.has_cycle);
        assert_eq!(analysis.node_total, 3);

        let sort = graph_topological_sort(&ctx, "gq-2").await.unwrap();
        assert!(!sort.sorted_nodes.is_empty());

        let reachability = graph_reachability(&ctx, "gq-2").await.unwrap();
        assert!(!reachability.reachable_from_start.is_empty());

        let registered = list_graph_workflows(&ctx).await.unwrap();
        assert!(registered.contains(&"gq-2".to_string()));
    }
}
