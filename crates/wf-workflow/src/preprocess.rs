use std::collections::{HashMap, HashSet};

use serde_json::Value;
use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

use crate::analysis::{analyze_graph, GraphAnalysis};
use crate::validation::{GraphValidator, ValidationError};

const MAX_EMBED_DEPTH: usize = 16;

/// An embedded subgraph extracted from an EMBED_GRAPH node.
#[derive(Debug, Clone)]
pub struct EmbedSubgraph {
    pub embed_node_id: String,
    /// The embedded graph after recursive embed expansion (un-namespaced).
    pub graph: WorkflowGraphStructure,
    pub validation_errors: Vec<ValidationError>,
    pub analysis: GraphAnalysis,
    /// Nested embeds of the embedded graph, recursively processed.
    pub nested_embeds: Vec<EmbedSubgraph>,
}

/// The preprocessed view of a workflow graph. `graph` is the *execution*
/// graph: every EMBED_GRAPH node has been expanded in place (START converted
/// to EMBED_START, END to EMBED_END, node ids namespaced with the embed node
/// id). Runtime executes exactly this structure, so the preprocessed graph
/// is the single source of truth for execution, analysis and validation.
#[derive(Debug, Clone)]
pub struct PreprocessedGraph {
    pub graph: WorkflowGraphStructure,
    pub validation_errors: Vec<ValidationError>,
    /// Non-blocking consistency warnings (e.g. structural cycles that cannot
    /// converge through loop state). Surfaced to logs, never reject the
    /// workflow.
    pub warnings: Vec<String>,
    pub analysis: GraphAnalysis,
    pub embeds: Vec<EmbedSubgraph>,
}

impl PreprocessedGraph {
    pub fn is_valid(&self) -> bool {
        self.validation_errors.is_empty()
    }
}

/// Extract the inline graph definition of an EMBED_GRAPH node config
/// (`graph_definition`), falling back to a registered graph referenced by
/// `embed_id`.
pub fn extract_embed_graph(node: &WorkflowNode) -> Option<WorkflowGraphStructure> {
    let inline = node.inner.get("graph_definition")?;
    if !inline.is_null() {
        if let Ok(sub) = serde_json::from_value(inline.clone()) {
            return Some(sub);
        }
    }
    let embed_id = node.inner.get("embed_id").and_then(|v| v.as_str())?;
    crate::registry::lookup_graph(embed_id)
}

/// Enforce the EMBED_GRAPH constraints on an embedded workflow definition:
/// it must not define variables, must not define triggers, and must not
/// contain VARIABLE nodes. Violations are reported as structured
/// validation errors that block execution.
pub fn validate_embed_graph_constraints(
    embed_node_id: &str,
    definition: &Value,
    sub: &WorkflowGraphStructure,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    let variables = definition.get("variables");
    if variables.is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty()) || v.is_object()) {
        errors.push(ValidationError::new(
            format!("nodes.{}", embed_node_id),
            format!(
                "EMBED_GRAPH node '{}' embeds a workflow that defines variables; embedded workflows cannot define variables (use SUBGRAPH for variable passing)",
                embed_node_id
            ),
        ));
    }

    let triggers = definition.get("triggers");
    if triggers.is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty())) {
        errors.push(ValidationError::new(
            format!("nodes.{}", embed_node_id),
            format!(
                "EMBED_GRAPH node '{}' embeds a workflow that defines triggers; embedded workflows cannot define triggers",
                embed_node_id
            ),
        ));
    }

    if sub.nodes.iter().any(|n| n.node_type == "VARIABLE") {
        errors.push(ValidationError::new(
            format!("nodes.{}", embed_node_id),
            format!(
                "EMBED_GRAPH node '{}' embeds a workflow that contains VARIABLE nodes; embedded workflows cannot contain VARIABLE nodes",
                embed_node_id
            ),
        ));
    }

    errors
}

/// Preprocess a workflow graph: validate every inline embedded subgraph
/// (recursively), enforce EMBED_GRAPH constraints, and produce the flattened
/// execution graph.
pub fn preprocess_graph(graph: WorkflowGraphStructure) -> PreprocessedGraph {
    preprocess_graph_inner(graph, 0, false)
}

fn preprocess_graph_inner(
    graph: WorkflowGraphStructure,
    depth: usize,
    validate_graph: bool,
) -> PreprocessedGraph {
    // Full graph validation runs on embedded subgraphs (they must be
    // standalone workflows) and is reported per-embed; the top-level graph's
    // validation is owned by the callers, because the coordinator also
    // executes branch/continuation subgraphs that are not standalone
    // workflows (fork branches, triggered continuations).
    let mut validation_errors = if validate_graph {
        GraphValidator::validate(graph.clone())
            .err()
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut embeds = Vec::new();
    if depth < MAX_EMBED_DEPTH {
        for node in &graph.nodes {
            if node.node_type != "EMBED_GRAPH" {
                continue;
            }
            if let Some(sub) = extract_embed_graph(node) {
                validation_errors.extend(validate_embed_graph_constraints(
                    &node.id,
                    node.inner.get("graph_definition").unwrap_or(&Value::Null),
                    &sub,
                ));
                let sub_pre = preprocess_graph_inner(sub, depth + 1, true);
                embeds.push(EmbedSubgraph {
                    embed_node_id: node.id.clone(),
                    graph: sub_pre.graph,
                    validation_errors: sub_pre.validation_errors,
                    analysis: sub_pre.analysis,
                    nested_embeds: sub_pre.embeds,
                });
            } else if node.inner.get("graph_definition").is_some() {
                validation_errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!(
                        "EMBED_GRAPH node '{}' has an invalid graph_definition",
                        node.id
                    ),
                ));
            } else {
                validation_errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!(
                        "EMBED_GRAPH node '{}' cannot be expanded: no graph_definition and no registered embed_id",
                        node.id
                    ),
                ));
            }
        }
    }

    let flattened = flatten_graph(&graph);
    let analysis = analyze_graph(&flattened);

    // Structural cycles that are not legal loops (a loop closes through the
    // LOOP_END -> LOOP_START back edge, which `detect_cycles` excludes):
    // such cycles never converge through loop state and can only be stopped
    // by the runtime navigation backstop. Reported as a warning, never a
    // rejection.
    let mut warnings = Vec::new();
    if analysis.cycle_detection.has_cycle {
        warnings.push(format!(
            "Non-loop cycle detected through nodes [{}]; execution depends on the navigation backstop (max_navigation_multiplier)",
            analysis.cycle_detection.cycle_nodes.join(", ")
        ));
    }

    PreprocessedGraph {
        graph: flattened,
        validation_errors,
        warnings,
        analysis,
        embeds,
    }
}

/// Remap internal node-id references and loop ids inside an embedded node's
/// config to their namespaced counterparts, so embedded control-flow nodes
/// (LOOP_END routing, ROUTE targets, FORK branch targets) keep working in
/// the flattened execution graph. Unknown references (pointing outside the
/// embed) are left untouched.
fn remap_node_references(inner: &mut Value, node_id_map: &HashMap<String, String>, prefix: &str) {
    let remap_id = |value: &mut Value| {
        if let Some(s) = value.as_str() {
            if let Some(mapped) = node_id_map.get(s) {
                *value = Value::String(mapped.clone());
            }
        }
    };

    // LOOP_START / LOOP_END: the loop id becomes namespaced so parent and
    // embedded loops cannot collide in the loop state stack.
    if let Some(v) = inner.get_mut("loop_id") {
        if let Some(s) = v.as_str() {
            *v = Value::String(format!("{}{}", prefix, s));
        }
    }
    // LOOP_END: routing hint back to the loop start node.
    if let Some(v) = inner.get_mut("loop_start_node_id") {
        remap_id(v);
    }
    // ROUTE: condition targets and the default target.
    if let Some(conditions) = inner.get_mut("conditions").and_then(|v| v.as_array_mut()) {
        for condition in conditions {
            if let Some(v) = condition.get_mut("target_node_id") {
                remap_id(v);
            }
        }
    }
    if let Some(v) = inner.get_mut("default_target_node_id") {
        remap_id(v);
    }
    // FORK: branch entry node ids.
    if let Some(paths) = inner.get_mut("fork_paths").and_then(|v| v.as_array_mut()) {
        for path in paths {
            if let Some(v) = path.get_mut("child_node_id") {
                remap_id(v);
            }
        }
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

/// Expand all EMBED_GRAPH nodes in place (recursively, depth-capped). Each
/// embedded subgraph is namespaced with `<embed_node_id>:`; the subgraph
/// START node is converted to EMBED_START and each END node to EMBED_END
/// (carrying `original_node_id`/`embed_node_id` metadata); edges touching
/// the EMBED_GRAPH node are rewired to the boundary nodes. The produced
/// structure is the execution graph: EMBED_GRAPH nodes themselves never run.
pub fn flatten_graph(graph: &WorkflowGraphStructure) -> WorkflowGraphStructure {
    flatten_graph_inner(graph, 0)
}

fn flatten_graph_inner(graph: &WorkflowGraphStructure, depth: usize) -> WorkflowGraphStructure {
    if depth >= MAX_EMBED_DEPTH {
        return graph.clone();
    }

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

        // Recursively expand nested embeds before namespacing.
        let sub = flatten_graph_inner(&sub, depth + 1);

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
            match n.node_type.as_str() {
                "START" => {
                    nn.node_type = "EMBED_START".to_string();
                    if !nn.inner.is_object() {
                        nn.inner = Value::Object(serde_json::Map::new());
                    }
                    nn.inner["original_node_id"] = Value::String(n.id.clone());
                    nn.inner["embed_node_id"] = Value::String(node.id.clone());
                }
                "END" => {
                    nn.node_type = "EMBED_END".to_string();
                    if !nn.inner.is_object() {
                        nn.inner = Value::Object(serde_json::Map::new());
                    }
                    nn.inner["original_node_id"] = Value::String(n.id.clone());
                    nn.inner["embed_node_id"] = Value::String(node.id.clone());
                }
                _ => {}
            }
            remap_node_references(&mut nn.inner, &node_id_map, &prefix);
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

        // START/END are converted to boundary node types.
        let type_of = |id: &str| -> String {
            flat.nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.node_type.clone())
                .unwrap_or_default()
        };
        assert_eq!(type_of("embed:s2"), "EMBED_START");
        assert_eq!(type_of("embed:e2"), "EMBED_END");
        assert_eq!(type_of("start"), "START");
        assert_eq!(type_of("end"), "END");

        // Boundary nodes carry the original-location metadata.
        let embed_start = flat.nodes.iter().find(|n| n.id == "embed:s2").unwrap();
        assert_eq!(
            embed_start.inner.get("original_node_id"),
            Some(&Value::String("s2".to_string()))
        );
        assert_eq!(
            embed_start.inner.get("embed_node_id"),
            Some(&Value::String("embed".to_string()))
        );

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
    fn flatten_expands_nested_embeds_recursively() {
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

        let flat = flatten_graph(&g);
        let ids: HashSet<String> = flat.nodes.iter().map(|n| n.id.clone()).collect();
        assert!(ids.contains("embed1:s2"));
        assert!(ids.contains("embed1:embed2:s3"));
        assert!(ids.contains("embed1:embed2:e3"));
        assert!(!ids.contains("embed1:embed2"), "nested embed expanded away");
        assert!(!ids.contains("embed2"), "nested embed expanded away");

        // The nested START/END are converted too.
        let type_of = |id: &str| -> String {
            flat.nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.node_type.clone())
                .unwrap_or_default()
        };
        assert_eq!(type_of("embed1:s2"), "EMBED_START");
        assert_eq!(type_of("embed1:embed2:s3"), "EMBED_START");
        assert_eq!(type_of("embed1:embed2:e3"), "EMBED_END");

        let analysis = analyze_graph(&flat);
        assert!(!analysis.cycle_detection.has_cycle);
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

        // The preprocessed graph is the flattened execution graph.
        assert!(pre.graph.nodes.iter().all(|n| n.node_type != "EMBED_GRAPH"));
        let type_of = |id: &str| -> String {
            pre.graph
                .nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.node_type.clone())
                .unwrap_or_default()
        };
        assert_eq!(type_of("embed1:embed2:s3"), "EMBED_START");
        assert!(!pre.analysis.cycle_detection.has_cycle);
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
    fn preprocess_rejects_embed_constraint_violations() {
        let plain_sub = graph(
            vec![node("s2", "START"), node("e2", "END")],
            vec![edge("s2", "e2")],
            Some("s2"),
            vec!["e2"],
        );

        // The graph_definition carries `variables`/`triggers` metadata
        // alongside the graph structure: both must be rejected.
        let mut definition = serde_json::to_value(&plain_sub).unwrap();
        if let Value::Object(map) = &mut definition {
            map.insert("variables".to_string(), serde_json::json!([{"name": "x"}]));
            map.insert(
                "triggers".to_string(),
                serde_json::json!([{"trigger_id": "t1"}]),
            );
        }
        let mut embed = embed_node("embed", &plain_sub);
        embed.inner["graph_definition"] = definition;

        let g = graph(
            vec![node("start", "START"), embed, node("end", "END")],
            vec![edge("start", "embed"), edge("embed", "end")],
            Some("start"),
            vec!["end"],
        );
        let pre = preprocess_graph(g);
        assert!(
            !pre.is_valid(),
            "constraint violations must block execution"
        );
        assert!(
            pre.validation_errors
                .iter()
                .any(|e| e.message.contains("defines variables")),
            "variables rule must be reported: {:?}",
            pre.validation_errors
        );
        assert!(
            pre.validation_errors
                .iter()
                .any(|e| e.message.contains("defines triggers")),
            "triggers rule must be reported: {:?}",
            pre.validation_errors
        );

        // An embedded workflow containing a VARIABLE node is rejected too.
        let with_variable_node = graph(
            vec![
                node("s2", "START"),
                node("v", "VARIABLE"),
                node("e2", "END"),
            ],
            vec![edge("s2", "v"), edge("v", "e2")],
            Some("s2"),
            vec!["e2"],
        );
        let g2 = graph(
            vec![
                node("start", "START"),
                embed_node("embed", &with_variable_node),
                node("end", "END"),
            ],
            vec![edge("start", "embed"), edge("embed", "end")],
            Some("start"),
            vec!["end"],
        );
        let pre2 = preprocess_graph(g2);
        assert!(!pre2.is_valid(), "VARIABLE nodes in embed must be rejected");
        assert!(
            pre2.validation_errors
                .iter()
                .any(|e| e.message.contains("VARIABLE nodes")),
            "VARIABLE rule must be reported: {:?}",
            pre2.validation_errors
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

    #[test]
    fn non_loop_cycle_emits_warning_but_stays_valid() {
        let g = graph(
            vec![
                node("start", "START"),
                node("a", "VARIABLE"),
                node("b", "VARIABLE"),
            ],
            vec![edge("start", "a"), edge("a", "b"), edge("b", "a")],
            Some("start"),
            vec![],
        );
        let pre = preprocess_graph(g);
        assert!(pre.is_valid(), "cycle warnings must not reject the graph");
        assert!(
            pre.warnings
                .iter()
                .any(|w| w.contains("Non-loop cycle") && w.contains("a") && w.contains("b")),
            "expected a non-loop cycle warning, got: {:?}",
            pre.warnings
        );
    }

    #[test]
    fn legal_loop_emits_no_cycle_warning() {
        let g = graph(
            vec![
                node("start", "START"),
                node("ls", "LOOP_START"),
                node("body", "VARIABLE"),
                node("le", "LOOP_END"),
                node("end", "END"),
            ],
            vec![
                edge("start", "ls"),
                edge("ls", "body"),
                edge("body", "le"),
                edge("le", "ls"),
                edge("le", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let pre = preprocess_graph(g);
        assert!(
            pre.warnings.is_empty(),
            "legal loop must not warn, got: {:?}",
            pre.warnings
        );
    }
}
