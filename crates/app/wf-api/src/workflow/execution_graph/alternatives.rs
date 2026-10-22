

use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub async fn get_alternative_paths(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<AlternativeDecision>> {
    let graph = super::analysis::resolve_graph(ctx, execution_id)
        .await?
        .unwrap_or_else(<WorkflowGraphStructure as super::paths::EmptyGraph>::default_empty);
    let executed_nodes = super::analysis::executed_nodes(ctx, execution_id).await;
    let decision_points = super::paths::analyze_decision_points(&graph, &executed_nodes);

    let mut alternatives = Vec::new();
    for point in &decision_points {
        let taken = point.taken_edge.clone();
        for edge in &graph.edges {
            if edge.source_node_id != point.node_id {
                continue;
            }
            if Some(edge.id.as_str()) == taken.as_deref() {
                continue;
            }
            let target_name = graph
                .nodes
                .iter()
                .find(|n| n.id == edge.target_node_id)
                .and_then(|n| n.name.clone())
                .unwrap_or_else(|| edge.target_node_id.clone());
            alternatives.push(AlternativeDecision {
                node_id: edge.target_node_id.clone(),
                node_name: Some(target_name.clone()),
                description: format!(
                    "Path via {} through edge '{}'",
                    target_name,
                    edge.label.clone().unwrap_or_else(|| edge.id.clone())
                ),
                reason: edge.condition.clone(),
                success_probability: None,
                confidence: None,
            });
        }
    }
    alternatives.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    Ok(alternatives)
}

