use std::collections::{HashMap, HashSet};


use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub async fn get_path_probability_analysis(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<PathProbabilityAnalysis> {
    let graph = super::analysis::resolve_graph(ctx, execution_id)
        .await?
        .unwrap_or_else(<WorkflowGraphStructure as super::paths::EmptyGraph>::default_empty);
    let paths = super::paths::enumerate_paths(&graph);
    if paths.is_empty() {
        return Ok(PathProbabilityAnalysis {
            execution_id: execution_id.to_string(),
            paths: Vec::new(),
            most_likely_path: None,
            least_likely_taken_path: None,
            path_diversity: 0.0,
        });
    }

    let executed_nodes = super::analysis::executed_nodes(ctx, execution_id).await;
    let executed_set: HashSet<&str> = executed_nodes.iter().map(String::as_str).collect();
    let edge_probability = edge_probabilities(&graph);

    let mut entries: Vec<PathProbabilityEntry> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let mut probability = 1.0;
            for window in path.nodes.windows(2) {
                let source = &window[0];
                let target = &window[1];
                probability *= edge_probability
                    .get(&(source.clone(), target.clone()))
                    .copied()
                    .unwrap_or(1.0);
            }
            let is_taken = path
                .nodes
                .iter()
                .all(|id| executed_set.contains(id.as_str()));
            PathProbabilityEntry {
                path_id: format!("path-{index}"),
                node_ids: path.nodes.clone(),
                probability: round3(probability),
                is_taken,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.probability
            .partial_cmp(&a.probability)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let most_likely_path = entries.first().map(|e| e.node_ids.clone());
    let taken_paths: Vec<&PathProbabilityEntry> = entries.iter().filter(|e| e.is_taken).collect();
    let least_likely_taken_path = taken_paths
        .iter()
        .min_by(|a, b| {
            a.probability
                .partial_cmp(&b.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|e| e.node_ids.clone());

    let total_probability: f64 = entries.iter().map(|e| e.probability).sum();
    let path_diversity = if total_probability > 0.0 && entries.len() > 1 {
        let entropy = -entries
            .iter()
            .map(|e| {
                let normalized = e.probability / total_probability;
                if normalized > 0.0 {
                    normalized * normalized.log2()
                } else {
                    0.0
                }
            })
            .sum::<f64>();
        round3(entropy / (entries.len() as f64).log2())
    } else {
        0.0
    };

    Ok(PathProbabilityAnalysis {
        execution_id: execution_id.to_string(),
        paths: entries,
        most_likely_path,
        least_likely_taken_path,
        path_diversity,
    })
}


fn edge_probabilities(graph: &WorkflowGraphStructure) -> HashMap<(String, String), f64> {
    let mut probabilities = HashMap::new();
    for node in &graph.nodes {
        let conditional: Vec<&wf_types::workflow_execution::WorkflowEdge> = graph
            .edges
            .iter()
            .filter(|e| e.source_node_id == node.id && e.condition.is_some())
            .collect();
        let share = if conditional.is_empty() {
            1.0
        } else {
            1.0 / conditional.len() as f64
        };
        for edge in graph.edges.iter().filter(|e| e.source_node_id == node.id) {
            probabilities.insert(
                (edge.source_node_id.clone(), edge.target_node_id.clone()),
                share,
            );
        }
    }
    probabilities
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
