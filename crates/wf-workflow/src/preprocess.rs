use std::collections::{HashMap, HashSet};

use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

use crate::analysis::{analyze_graph, GraphAnalysis};
use crate::validation::{GraphValidator, ValidationError};

const MAX_EMBED_DEPTH: usize = 16;

/// An embedded subgraph extracted from an EMBED_GRAPH node.
#[derive(Debug, Clone)]
pub struct EmbedSubgraph {
    pub embed_node_id: String,
    pub graph: WorkflowGraphStructure,
    pub validation_errors: Vec<ValidationError>,
    pub analysis: GraphAnalysis,
    /// Nested embeds of the embedded graph, recursively processed.
    pub nested_embeds: Vec<EmbedSubgraph>,
}

/// The preprocessed view of a workflow graph: the original graph plus the
/// results of validation, analysis and embed expansion.
#[derive(Debug, Clone)]
pub struct PreprocessedGraph {
    pub graph: WorkflowGraphStructure,
    pub validation_errors: Vec<ValidationError>,
    pub analysis: GraphAnalysis,
    pub embeds: Vec<EmbedSubgraph>,
    /// EMBED_GRAPH nodes expanded in place. Cross-boundary cycle and
    /// reachability analysis should run on this view.
    pub flattened: WorkflowGraphStructure,
}

impl PreprocessedGraph {
    pub fn is_valid(&self) -> bool {
        self.validation_errors.is_empty()
    }
}

/// Extract the inline graph definition of an EMBED_GRAPH node config
/// (`graph_definition`).
pub fn extract_embed_graph(node: &WorkflowNode) -> Option<WorkflowGraphStructure> {
    serde_json::from_value(node.inner.get("graph_definition")?.clone()).ok()
}

/// Preprocess a workflow graph: run validation and analysis on the graph and
/// on every inline embedded subgraph (recursively), and produce the flattened
/// view used for cross-boundary analysis.
pub fn preprocess_graph(graph: WorkflowGraphStructure) -> PreprocessedGraph {
    preprocess_graph_inner(graph, 0)
}

fn preprocess_graph_inner(graph: WorkflowGraphStructure, depth: usize) -> PreprocessedGraph {
    let validation_errors = GraphValidator::validate(&graph).err().unwrap_or_default();
    let analysis = analyze_graph(&graph);

    let mut embeds = Vec::new();
    if depth < MAX_EMBED_DEPTH {
        for node in &graph.nodes {
            if node.node_type != "EMBED_GRAPH" {
                continue;
            }
            if let Some(subgraph) = extract_embed_graph(node) {
                let sub = preprocess_graph_inner(subgraph, depth + 1);
                embeds.push(EmbedSubgraph {
                    embed_node_id: node.id.clone(),
                    graph: sub.graph,
                    validation_errors: sub.validation_errors,
                    analysis: sub.analysis,
                    nested_embeds: sub.embeds,
                });
            }
        }
    }

    let flattened = flatten_graph(&graph);

    PreprocessedGraph {
        graph,
        validation_errors,
        analysis,
        embeds,
        flattened,
    }
}

/// Entry points of an embedded graph: the declared start node, falling back
/// to nodes with in-degree 0.
fn entry_nodes(sub: &WorkflowGraphStructure) -> Vec<String> {
    if let Some(ref start) = sub.start_node_id {
        if sub.nodes.iter().any(|n| &n.id == start) {
            return vec![start.clone()];
        }
    }
    let has_incoming: HashSet<&str> = sub
        .edges
        .iter()
        .map(|e| e.target_node_id.as_str())
        .collect();
    sub.nodes
        .iter()
        .filter(|n| !has_incoming.contains(n.id.as_str()))
        .map(|n| n.id.clone())
        .collect()
}

/// Exit points of an embedded graph: the declared end nodes, falling back to
/// nodes with out-degree 0.
fn exit_nodes(sub: &WorkflowGraphStructure) -> Vec<String> {
    if !sub.end_node_ids.is_empty()
        && sub
            .end_node_ids
            .iter()
            .all(|id| sub.nodes.iter().any(|n| &n.id == id))
    {
        return sub.end_node_ids.clone();
    }
    let has_outgoing: HashSet<&str> = sub
        .edges
        .iter()
        .map(|e| e.source_node_id.as_str())
        .collect();
    sub.nodes
        .iter()
        .filter(|n| !has_outgoing.contains(n.id.as_str()))
        .map(|n| n.id.clone())
        .collect()
}

struct EmbedExpansion {
    node_id: String,
    entry_ids: Vec<String>,
    exit_ids: Vec<String>,
    nodes: Vec<WorkflowNode>,
    edges: Vec<WorkflowEdge>,
}

/// Expand all EMBED_GRAPH nodes in place. Each embedded subgraph is
/// namespaced with `<embed_node_id>:`; edges touching the EMBED_GRAPH node
/// are rewired to the subgraph entry/exit points. The expansion is an
/// analysis view: runtime execution still resolves embeds through the
/// EmbedHandler.
pub fn flatten_graph(graph: &WorkflowGraphStructure) -> WorkflowGraphStructure {
    let mut expansions: Vec<EmbedExpansion> = Vec::new();

    for node in &graph.nodes {
        if node.node_type != "EMBED_GRAPH" {
            continue;
        }
        let Some(sub) = extract_embed_graph(node) else {
            continue;
        };
        if sub.nodes.is_empty() {
            continue;
        }

        let prefix = format!("{}:", node.id);
        let node_id_map: HashMap<String, String> = sub
            .nodes
            .iter()
            .map(|n| (n.id.clone(), format!("{}{}", prefix, n.id)))
            .collect();

        let mut sub_nodes: Vec<WorkflowNode> = Vec::new();
        for n in &sub.nodes {
            let mut nn = n.clone();
            nn.id = node_id_map[&n.id].clone();
            sub_nodes.push(nn);
        }

        let mut sub_edges: Vec<WorkflowEdge> = Vec::new();
        for e in &sub.edges {
            let mut ne = e.clone();
            ne.id = format!("{}{}", prefix, e.id);
            ne.source_node_id = node_id_map
                .get(&e.source_node_id)
                .cloned()
                .unwrap_or_default();
            ne.target_node_id = node_id_map
                .get(&e.target_node_id)
                .cloned()
                .unwrap_or_default();
            sub_edges.push(ne);
        }

        expansions.push(EmbedExpansion {
            node_id: node.id.clone(),
            entry_ids: entry_nodes(&sub)
                .into_iter()
                .filter_map(|id| node_id_map.get(&id).cloned())
                .collect(),
            exit_ids: exit_nodes(&sub)
                .into_iter()
                .filter_map(|id| node_id_map.get(&id).cloned())
                .collect(),
            nodes: sub_nodes,
            edges: sub_edges,
        });
    }

    let embed_ids: HashSet<&str> = expansions.iter().map(|e| e.node_id.as_str()).collect();

    let mut nodes: Vec<WorkflowNode> = graph
        .nodes
        .iter()
        .filter(|n| !embed_ids.contains(n.id.as_str()))
        .cloned()
        .collect();
    for expansion in &expansions {
        nodes.extend(expansion.nodes.clone());
    }

    let mut edges: Vec<WorkflowEdge> = Vec::new();
    for expansion in &expansions {
        edges.extend(expansion.edges.clone());
    }
    for edge in &graph.edges {
        let source_is_embed = embed_ids.contains(edge.source_node_id.as_str());
        let target_is_embed = embed_ids.contains(edge.target_node_id.as_str());

        if source_is_embed && target_is_embed {
            let Some(expansion) = expansions.iter().find(|e| e.node_id == edge.source_node_id)
            else {
                continue;
            };
            for exit in &expansion.exit_ids {
                for entry in &expansion.entry_ids {
                    let mut ne = edge.clone();
                    ne.id = format!("{}-{}", edge.id, exit);
                    ne.source_node_id = exit.clone();
                    ne.target_node_id = entry.clone();
                    edges.push(ne);
                }
            }
        } else if target_is_embed {
            let Some(expansion) = expansions.iter().find(|e| e.node_id == edge.target_node_id)
            else {
                continue;
            };
            for entry in &expansion.entry_ids {
                let mut ne = edge.clone();
                ne.id = format!("{}-{}", edge.id, entry);
                ne.target_node_id = entry.clone();
                edges.push(ne);
            }
        } else if source_is_embed {
            let Some(expansion) = expansions.iter().find(|e| e.node_id == edge.source_node_id)
            else {
                continue;
            };
            for exit in &expansion.exit_ids {
                let mut ne = edge.clone();
                ne.id = format!("{}-{}", edge.id, exit);
                ne.source_node_id = exit.clone();
                edges.push(ne);
            }
        } else {
            edges.push(edge.clone());
        }
    }

    let mut start_node_id = graph.start_node_id.clone();
    if let Some(ref start) = graph.start_node_id {
        if let Some(expansion) = expansions.iter().find(|e| e.node_id == *start) {
            start_node_id = expansion.entry_ids.first().cloned();
        }
    }

    let mut end_node_ids: Vec<String> = Vec::new();
    for end_id in &graph.end_node_ids {
        if let Some(expansion) = expansions.iter().find(|e| e.node_id == *end_id) {
            end_node_ids.extend(expansion.exit_ids.clone());
        } else {
            end_node_ids.push(end_id.clone());
        }
    }

    WorkflowGraphStructure {
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id,
        end_node_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wf_types::workflow::EdgeType;

    fn node(id: &str, node_type: &str) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner: serde_json::json!({}),
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
        }
    }

    fn graph(
        nodes: Vec<WorkflowNode>,
        edges: Vec<WorkflowEdge>,
        start: Option<&str>,
        ends: Vec<&str>,
    ) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges,
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: start.map(String::from),
            end_node_ids: ends.into_iter().map(String::from).collect(),
        }
    }

    fn embed_node(id: &str, sub: &WorkflowGraphStructure) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: "EMBED_GRAPH".to_string(),
            inner: serde_json::json!({ "graph_definition": sub }),
        }
    }

    #[test]
    fn flatten_inlines_embed_subgraph() {
        let sub = graph(
            vec![
                node("s2", "START"),
                node("w", "VARIABLE"),
                node("e2", "END"),
            ],
            vec![edge("s2", "w"), edge("w", "e2")],
            Some("s2"),
            vec!["e2"],
        );
        let g = graph(
            vec![
                node("start", "START"),
                embed_node("embed", &sub),
                node("end", "END"),
            ],
            vec![edge("start", "embed"), edge("embed", "end")],
            Some("start"),
            vec!["end"],
        );

        let flat = flatten_graph(&g);

        let ids: HashSet<String> = flat.nodes.iter().map(|n| n.id.clone()).collect();
        assert!(ids.contains("start"));
        assert!(ids.contains("end"));
        assert!(ids.contains("embed:s2"));
        assert!(ids.contains("embed:w"));
        assert!(ids.contains("embed:e2"));
        assert!(!ids.contains("embed"), "embed node must be expanded away");

        // start -> embed:s2 -> embed:w -> embed:e2 -> end
        let starts = flat
            .edges
            .iter()
            .filter(|e| e.source_node_id == "start")
            .map(|e| e.target_node_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(starts, vec!["embed:s2".to_string()]);

        let ends = flat
            .edges
            .iter()
            .filter(|e| e.target_node_id == "end")
            .map(|e| e.source_node_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(ends, vec!["embed:e2".to_string()]);

        assert_eq!(flat.start_node_id.as_deref(), Some("start"));
        assert_eq!(flat.end_node_ids, vec!["end".to_string()]);
    }

    #[test]
    fn flatten_keeps_plain_graph_unchanged() {
        let g = graph(
            vec![node("start", "START"), node("end", "END")],
            vec![edge("start", "end")],
            Some("start"),
            vec!["end"],
        );
        let flat = flatten_graph(&g);
        assert_eq!(flat.nodes.len(), 2);
        assert_eq!(flat.edges.len(), 1);
        assert_eq!(flat.start_node_id, g.start_node_id);
        assert_eq!(flat.end_node_ids, g.end_node_ids);
    }

    #[test]
    fn preprocess_validates_embeds_recursively() {
        let inner = graph(
            vec![node("s3", "START"), node("e3", "END")],
            vec![edge("s3", "e3")],
            Some("s3"),
            vec!["e3"],
        );
        let mid = graph(
            vec![
                node("s2", "START"),
                embed_node("embed2", &inner),
                node("e2", "END"),
            ],
            vec![edge("s2", "embed2"), edge("embed2", "e2")],
            Some("s2"),
            vec!["e2"],
        );
        let g = graph(
            vec![
                node("start", "START"),
                embed_node("embed1", &mid),
                node("end", "END"),
            ],
            vec![edge("start", "embed1"), edge("embed1", "end")],
            Some("start"),
            vec!["end"],
        );

        let pre = preprocess_graph(g);
        assert!(
            pre.is_valid(),
            "valid graph with nested embeds: {:?}",
            pre.validation_errors
        );
        assert_eq!(pre.embeds.len(), 1);
        assert_eq!(pre.embeds[0].nested_embeds.len(), 1);

        // Flattened view must be valid and acyclic.
        let flat_analysis = analyze_graph(&pre.flattened);
        assert!(!flat_analysis.cycle_detection.has_cycle);
    }

    #[test]
    fn preprocess_reports_invalid_embed() {
        // The embedded graph has an LLM node without a profile.
        let bad_sub = graph(
            vec![node("s2", "START"), node("llm", "LLM"), node("e2", "END")],
            vec![edge("s2", "llm"), edge("llm", "e2")],
            Some("s2"),
            vec!["e2"],
        );
        let g = graph(
            vec![
                node("start", "START"),
                embed_node("embed", &bad_sub),
                node("end", "END"),
            ],
            vec![edge("start", "embed"), edge("embed", "end")],
            Some("start"),
            vec!["end"],
        );

        let pre = preprocess_graph(g);
        assert!(pre.is_valid(), "parent graph itself is valid");
        assert_eq!(pre.embeds.len(), 1);
        assert!(
            pre.embeds[0]
                .validation_errors
                .iter()
                .any(|e| e.message.contains("profile_id")),
            "embed validation errors must be collected: {:?}",
            pre.embeds[0].validation_errors
        );
    }

    #[test]
    fn deep_embed_recursion_is_capped() {
        let mut sub = graph(
            vec![node("s2", "START"), node("e2", "END")],
            vec![edge("s2", "e2")],
            Some("s2"),
            vec!["e2"],
        );
        for _ in 0..(MAX_EMBED_DEPTH + 2) {
            let inner = std::mem::replace(&mut sub, graph(vec![], vec![], None, vec![]));
            sub = graph(
                vec![
                    node("sx", "START"),
                    WorkflowNode {
                        id: "embed".to_string(),
                        name: None,
                        node_type: "EMBED_GRAPH".to_string(),
                        inner: serde_json::json!({ "graph_definition": inner }),
                    },
                    node("ex", "END"),
                ],
                vec![edge("sx", "embed"), edge("embed", "ex")],
                Some("sx"),
                vec!["ex"],
            );
        }
        let pre = preprocess_graph(sub);
        // Depth cap must stop recursion without hanging; validation of the
        // outermost graph still completes.
        let mut depth = 0;
        let mut current: &[EmbedSubgraph] = &pre.embeds;
        while !current.is_empty() {
            depth += 1;
            current = &current[0].nested_embeds;
        }
        assert!(
            depth <= MAX_EMBED_DEPTH,
            "embed recursion must be capped, got depth {}",
            depth
        );
    }
}
