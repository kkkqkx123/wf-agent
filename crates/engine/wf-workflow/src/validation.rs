use std::collections::{HashMap, HashSet};

use wf_llm::ProfileManager;
use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

use crate::analysis::{analyze_graph, analyze_reachability, detect_cycles, get_reachable_nodes};
use crate::node_validation::validate_node_configs;
use crate::protocol_consistency::validate_protocol_consistency_with;
use crate::reference_closure::{ReferenceClosureReport, ReferenceContext};

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// A workflow graph that has passed structural validation.
///
/// This type can only be constructed through [`GraphValidator::validate`]
/// or [`GraphValidator::validate_with_profiles`], guaranteeing that an
/// instance represents a structurally valid workflow graph.
#[derive(Debug, Clone)]
pub struct ValidatedGraph(WorkflowGraphStructure);

impl ValidatedGraph {
    /// Access the underlying validated graph structure.
    pub fn inner(&self) -> &WorkflowGraphStructure {
        &self.0
    }

    /// Consume the wrapper and return the validated graph.
    pub fn into_inner(self) -> WorkflowGraphStructure {
        self.0
    }
}

pub type ValidatedGraphResult = Result<ValidatedGraph, Vec<ValidationError>>;

/// Render a validation error list as a human-readable report, one finding
/// per line with its field path. Used by CLI/TUI registration output.
pub fn format_validation_report(errors: &[ValidationError]) -> String {
    let mut report = format!("{} error(s) found:", errors.len());
    for error in errors {
        report.push_str(&format!("\n  - [{}] {}", error.field, error.message));
    }
    report
}

fn node_type_of<'a>(graph: &'a WorkflowGraphStructure, node_id: &str) -> Option<&'a str> {
    graph
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .map(|n| n.node_type.as_str())
}

fn outgoing_edges<'a>(graph: &'a WorkflowGraphStructure, node_id: &str) -> Vec<&'a WorkflowEdge> {
    graph
        .edges
        .iter()
        .filter(|e| e.source_node_id == node_id)
        .collect()
}

fn incoming_edges<'a>(graph: &'a WorkflowGraphStructure, node_id: &str) -> Vec<&'a WorkflowEdge> {
    graph
        .edges
        .iter()
        .filter(|e| e.target_node_id == node_id)
        .collect()
}

/// Extract the path ids of a FORK node config (`fork_paths[].path_id`).
fn fork_path_ids(node: &WorkflowNode) -> Vec<String> {
    node.inner
        .get("fork_paths")
        .and_then(|v| v.as_array())
        .map(|paths| {
            paths
                .iter()
                .filter_map(|p| p.get("path_id").and_then(|v| v.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Join node config path ids (`fork_path_ids`).
fn join_path_ids(node: &WorkflowNode) -> Vec<String> {
    node.inner
        .get("fork_path_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub struct GraphValidator;

impl GraphValidator {
    /// Validate a workflow graph against all graph-level and node-level
    /// rules. Runs before execution; an invalid graph is rejected with a
    /// structured error list.
    ///
    /// Returns a `ValidatedGraph` on success, which can only be constructed
    /// through this function, guaranteeing that the graph has been validated.
    pub fn validate(graph: WorkflowGraphStructure) -> ValidatedGraphResult {
        Self::validate_with_profiles(graph, None)
    }

    /// Profile-aware validation: pass the registered LLM profile set so that
    /// every `profile_id` reference is checked and node formats are checked
    /// against the referenced profile formats.
    pub fn validate_with_profiles(
        graph: WorkflowGraphStructure,
        profiles: Option<&ProfileManager>,
    ) -> ValidatedGraphResult {
        let mut errors: Vec<ValidationError> = Vec::new();
        errors.extend(Self::validate_nodes(&graph));
        errors.extend(Self::validate_edges(&graph));
        errors.extend(Self::validate_start_end(&graph));
        errors.extend(Self::validate_references(&graph));
        errors.extend(Self::validate_start_end_topology(&graph));
        errors.extend(Self::validate_isolated_nodes(&graph));
        errors.extend(Self::validate_fork_join_pairs(&graph));
        errors.extend(Self::validate_loop_pairs(&graph));
        errors.extend(Self::validate_sync_nodes(&graph));
        errors.extend(Self::validate_embed_graph(&graph));
        errors.extend(Self::validate_subgraph_nodes(&graph));
        errors.extend(Self::validate_triggered_subgraph(&graph));
        errors.extend(Self::validate_route_targets(&graph));
        errors.extend(Self::validate_fork_children(&graph));
        errors.extend(Self::validate_cycles(&graph));
        errors.extend(Self::validate_reachability(&graph));
        errors.extend(validate_node_configs(&graph));
        errors.extend(validate_protocol_consistency_with(&graph, profiles));

        if errors.is_empty() {
            Ok(ValidatedGraph(graph))
        } else {
            Err(errors)
        }
    }

    /// Formal validation with an assembled reference context: shape, graph
    /// and external reference closure in one pass. Warnings never block;
    /// they are returned alongside the validated graph for the caller report.
    pub fn validate_with_reference_context(
        graph: WorkflowGraphStructure,
        ctx: &ReferenceContext,
    ) -> Result<(ValidatedGraph, Vec<ValidationError>), Vec<ValidationError>> {
        let mut errors: Vec<ValidationError> = Vec::new();
        errors.extend(Self::validate_nodes(&graph));
        errors.extend(Self::validate_edges(&graph));
        errors.extend(Self::validate_start_end(&graph));
        errors.extend(Self::validate_references(&graph));
        errors.extend(Self::validate_start_end_topology(&graph));
        errors.extend(Self::validate_isolated_nodes(&graph));
        errors.extend(Self::validate_fork_join_pairs(&graph));
        errors.extend(Self::validate_loop_pairs(&graph));
        errors.extend(Self::validate_sync_nodes(&graph));
        errors.extend(Self::validate_embed_graph(&graph));
        errors.extend(Self::validate_subgraph_nodes(&graph));
        errors.extend(Self::validate_triggered_subgraph(&graph));
        errors.extend(Self::validate_route_targets(&graph));
        errors.extend(Self::validate_fork_children(&graph));
        errors.extend(Self::validate_cycles(&graph));
        errors.extend(Self::validate_reachability(&graph));
        errors.extend(validate_node_configs(&graph));
        let report: ReferenceClosureReport =
            crate::reference_closure::validate_reference_closure(&graph, ctx);
        errors.extend(report.errors.clone());
        let warnings = report.warnings;
        if errors.is_empty() {
            Ok((ValidatedGraph(graph), warnings))
        } else {
            Err(errors)
        }
    }

    /// Complete structural analysis (cycle detection, topological sort,
    /// reachability) without validation semantics.
    pub fn analyze(graph: &WorkflowGraphStructure) -> crate::analysis::GraphAnalysis {
        analyze_graph(graph)
    }

    fn validate_nodes(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let mut node_ids = HashSet::new();

        if graph.nodes.is_empty() {
            errors.push(ValidationError::new(
                "nodes",
                "Graph must have at least one node",
            ));
            return errors;
        }

        for node in &graph.nodes {
            if node.id.is_empty() {
                errors.push(ValidationError::new("nodes", "Node ID cannot be empty"));
            }
            if node.node_type.is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!("Node '{}' has no type", node.id),
                ));
            }
            if !node_ids.insert(node.id.clone()) {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!("Duplicate node ID: {}", node.id),
                ));
            }
        }

        errors
    }

    fn validate_edges(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let mut edge_ids = HashSet::new();

        for edge in &graph.edges {
            if edge.id.is_empty() {
                errors.push(ValidationError::new("edges", "Edge ID cannot be empty"));
            }
            if !edge_ids.insert(edge.id.clone()) {
                errors.push(ValidationError::new(
                    format!("edges.{}", edge.id),
                    format!("Duplicate edge ID: {}", edge.id),
                ));
            }
        }

        errors
    }

    fn validate_start_end(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let node_types: HashMap<&str, &str> = graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.node_type.as_str()))
            .collect();

        let start_count = node_types.values().filter(|t| **t == "START").count();
        let end_count = node_types.values().filter(|t| **t == "END").count();
        let message_start_count = node_types
            .values()
            .filter(|t| **t == "START_FROM_MESSAGE")
            .count();
        let message_end_count = node_types
            .values()
            .filter(|t| **t == "CONTINUE_FROM_MESSAGE")
            .count();

        let has_special = message_start_count > 0 || message_end_count > 0;

        if has_special {
            if message_start_count != 1 {
                errors.push(ValidationError::new(
                    "nodes",
                    "Triggered subgraph must have exactly one START_FROM_MESSAGE node",
                ));
            }
            if message_end_count != 1 {
                errors.push(ValidationError::new(
                    "nodes",
                    "Triggered subgraph must have exactly one CONTINUE_FROM_MESSAGE node",
                ));
            }
            if start_count > 0 {
                errors.push(ValidationError::new(
                    "nodes",
                    "Triggered subgraph cannot contain START node",
                ));
            }
            if end_count > 0 {
                errors.push(ValidationError::new(
                    "nodes",
                    "Triggered subgraph cannot contain END node",
                ));
            }
        } else {
            if start_count == 0 {
                errors.push(ValidationError::new(
                    "nodes",
                    "Workflow must have a START node",
                ));
            } else if start_count > 1 {
                errors.push(ValidationError::new(
                    "nodes",
                    "Workflow must have exactly one START node",
                ));
            }

            if end_count == 0 {
                errors.push(ValidationError::new(
                    "nodes",
                    "Workflow must have at least one END node",
                ));
            }
        }

        if let Some(ref start_id) = graph.start_node_id {
            if !node_types.contains_key(start_id.as_str()) {
                errors.push(ValidationError::new(
                    "start_node_id",
                    format!("Start node '{}' not found in nodes", start_id),
                ));
            }
        } else if !has_special {
            errors.push(ValidationError::new(
                "start_node_id",
                "Graph must have a start_node_id",
            ));
        }

        errors
    }

    fn validate_references(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let node_ids: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();

        for edge in &graph.edges {
            if !node_ids.contains(edge.source_node_id.as_str()) {
                errors.push(ValidationError::new(
                    format!("edges.{}", edge.id),
                    format!("Edge source '{}' not found in nodes", edge.source_node_id),
                ));
            }
            if !node_ids.contains(edge.target_node_id.as_str()) {
                errors.push(ValidationError::new(
                    format!("edges.{}", edge.id),
                    format!("Edge target '{}' not found in nodes", edge.target_node_id),
                ));
            }
        }

        errors
    }

    /// Topological constraints of boundary nodes: START cannot have incoming
    /// edges, END cannot have outgoing edges, and the triggered-subgraph
    /// boundaries follow the same rule.
    fn validate_start_end_topology(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if let Some(ref start_id) = graph.start_node_id {
            if node_type_of(graph, start_id).is_some()
                && !incoming_edges(graph, start_id).is_empty()
            {
                errors.push(ValidationError::new(
                    format!("nodes.{}", start_id),
                    "START node cannot have incoming edges",
                ));
            }
        }

        for end_id in &graph.end_node_ids {
            if node_type_of(graph, end_id).is_some() && !outgoing_edges(graph, end_id).is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", end_id),
                    format!("END node ({}) cannot have outgoing edges", end_id),
                ));
            }
        }

        for node in &graph.nodes {
            match node.node_type.as_str() {
                "START_FROM_MESSAGE" if !incoming_edges(graph, &node.id).is_empty() => {
                    errors.push(ValidationError::new(
                        format!("nodes.{}", node.id),
                        "START_FROM_MESSAGE node cannot have incoming edges",
                    ));
                }
                "CONTINUE_FROM_MESSAGE" if !outgoing_edges(graph, &node.id).is_empty() => {
                    errors.push(ValidationError::new(
                        format!("nodes.{}", node.id),
                        "CONTINUE_FROM_MESSAGE node cannot have outgoing edges",
                    ));
                }
                _ => {}
            }
        }

        errors
    }

    /// Boundary nodes are excluded; any other node without both an incoming
    /// and an outgoing edge is reported as isolated.
    fn validate_isolated_nodes(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for node in &graph.nodes {
            if matches!(
                node.node_type.as_str(),
                "START" | "END" | "START_FROM_MESSAGE" | "CONTINUE_FROM_MESSAGE"
            ) {
                continue;
            }
            if incoming_edges(graph, &node.id).is_empty()
                && outgoing_edges(graph, &node.id).is_empty()
            {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!(
                        "Node ({}) is isolated, has no incoming or outgoing edges",
                        node.id
                    ),
                ));
            }
        }

        errors
    }

    /// FORK/JOIN pairing: every FORK must have branches and a matching JOIN,
    /// every JOIN must match a FORK; paired path ids must agree as sets with
    /// matching counts; empty or duplicate path ids fail; the JOIN must be
    /// reachable from its FORK. Threshold compatibility for `wait_for_n`
    /// lives in the node-level JOIN validator where the path count is known.
    fn validate_fork_join_pairs(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let fork_nodes: Vec<&WorkflowNode> = graph
            .nodes
            .iter()
            .filter(|n| n.node_type == "FORK")
            .collect();
        let join_nodes: Vec<&WorkflowNode> = graph
            .nodes
            .iter()
            .filter(|n| n.node_type == "JOIN")
            .collect();

        // Empty path ids fail with a dedicated message before uniqueness.
        for fork in &fork_nodes {
            for (idx, raw) in fork
                .inner
                .get("fork_paths")
                .and_then(|v| v.as_array())
                .map(Vec::as_slice)
                .unwrap_or(&[])
                .iter()
                .enumerate()
            {
                let empty = raw
                    .get("path_id")
                    .and_then(|v| v.as_str())
                    .is_none_or(|s| s.trim().is_empty());
                if empty {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.fork_paths[{}].path_id", fork.id, idx),
                        format!(
                            "FORK node '{}' has an empty path_id; each branch needs a non-empty unique path id",
                            fork.id
                        ),
                    ));
                }
            }
        }
        for join in &join_nodes {
            if let Some(arr) = join.inner.get("fork_path_ids").and_then(|v| v.as_array()) {
                for (idx, entry) in arr.iter().enumerate() {
                    if entry.as_str().is_none_or(|s| s.trim().is_empty()) {
                        errors.push(ValidationError::new(
                            format!("nodes.{}.config.fork_path_ids[{}]", join.id, idx),
                            format!(
                                "JOIN node '{}' has an empty path id; each entry must be a non-empty string",
                                join.id
                            ),
                        ));
                    }
                }
            }
        }

        // Global path id uniqueness across all FORK nodes.
        let mut all_path_ids: HashSet<String> = HashSet::new();
        for fork in &fork_nodes {
            for path_id in fork_path_ids(fork) {
                if path_id.trim().is_empty() {
                    continue;
                }
                if !all_path_ids.insert(path_id.clone()) {
                    errors.push(ValidationError::new(
                        format!("nodes.{}", fork.id),
                        format!(
                            "pathId ({}) of FORK node ({}) is not unique within the workflow definition",
                            path_id, fork.id
                        ),
                    ));
                }
            }
        }

        // FORK nodes must declare branches and have outgoing edges.
        for fork in &fork_nodes {
            let outgoing = outgoing_edges(graph, &fork.id);
            if outgoing.is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", fork.id),
                    format!("FORK node '{}' has no outgoing edges", fork.id),
                ));
            }

            let branches = fork.inner.get("fork_paths").and_then(|b| b.as_array());
            match branches {
                None => errors.push(ValidationError::new(
                    format!("nodes.{}", fork.id),
                    format!(
                        "FORK node '{}' must define a non-empty fork_paths array",
                        fork.id
                    ),
                )),
                Some(branches) if branches.is_empty() => errors.push(ValidationError::new(
                    format!("nodes.{}", fork.id),
                    format!("FORK node '{}' has empty fork_paths", fork.id),
                )),
                _ => {}
            }
        }

        // JOIN nodes must have incoming edges.
        for join in &join_nodes {
            if incoming_edges(graph, &join.id).is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", join.id),
                    format!("JOIN node '{}' has no incoming edges", join.id),
                ));
            }
        }

        // Pairing by the first fork path id.
        let join_by_first_path: HashMap<String, &WorkflowNode> = join_nodes
            .iter()
            .filter_map(|j| join_path_ids(j).into_iter().next().map(|first| (first, *j)))
            .collect();

        let mut pairs: Vec<(&WorkflowNode, &WorkflowNode)> = Vec::new();
        let mut paired_joins: HashSet<&str> = HashSet::new();

        for fork in &fork_nodes {
            let path_ids = fork_path_ids(fork);
            let matched: Option<&WorkflowNode> = path_ids
                .first()
                .and_then(|first| join_by_first_path.get(first).copied());

            match matched {
                Some(join) => {
                    let join_ids = join_path_ids(join);
                    let fork_ids = fork_path_ids(fork);
                    if !fork_ids.is_empty() && !join_ids.is_empty() {
                        let mut sorted_fork = fork_ids.clone();
                        let mut sorted_join = join_ids.clone();
                        sorted_fork.sort();
                        sorted_join.sort();
                        if sorted_fork != sorted_join {
                            errors.push(ValidationError::new(
                                format!("nodes.{}", fork.id),
                                format!(
                                    "fork_path_ids of FORK node ({}) and JOIN node ({}) do not match: FORK has {} path(s) [{}], JOIN has {} path(s) [{}]",
                                    fork.id,
                                    join.id,
                                    fork_ids.len(),
                                    fork_ids.join(", "),
                                    join_ids.len(),
                                    join_ids.join(", "),
                                ),
                            ));
                        } else {
                            pairs.push((fork, join));
                            paired_joins.insert(join.id.as_str());
                        }
                    } else {
                        pairs.push((fork, join));
                        paired_joins.insert(join.id.as_str());
                    }
                }
                None => {
                    errors.push(ValidationError::new(
                        format!("nodes.{}", fork.id),
                        format!("FORK node ({}) has no matching JOIN node", fork.id),
                    ));
                }
            }
        }

        for join in &join_nodes {
            if !paired_joins.contains(join.id.as_str()) {
                errors.push(ValidationError::new(
                    format!("nodes.{}", join.id),
                    format!("JOIN node ({}) has no matching FORK node", join.id),
                ));
            }
        }

        // Reachability from FORK to its paired JOIN.
        for (fork, join) in &pairs {
            let reachable = get_reachable_nodes(graph, &fork.id);
            if !reachable.contains(&join.id) {
                errors.push(ValidationError::new(
                    format!("nodes.{}", fork.id),
                    format!(
                        "FORK node ({}) cannot reach the paired JOIN node ({})",
                        fork.id, join.id
                    ),
                ));
            }
        }

        errors
    }

    /// LOOP_START/LOOP_END pairing: each loopId must have exactly one of
    /// each, cross references must resolve, and boundary nodes must have
    /// edges on the required sides.
    fn validate_loop_pairs(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let mut loop_starts: HashMap<String, String> = HashMap::new(); // node_id -> loop_id
        let mut loop_ends: HashMap<String, (String, Option<String>)> = HashMap::new(); // node_id -> (loop_id, loop_start_node_id)

        for node in &graph.nodes {
            match node.node_type.as_str() {
                "LOOP_START" => {
                    let loop_id = node
                        .inner
                        .get("loop_id")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .filter(|s| !s.is_empty());
                    match loop_id {
                        Some(id) => {
                            loop_starts.insert(node.id.clone(), id);
                        }
                        None => errors.push(ValidationError::new(
                            format!("nodes.{}", node.id),
                            format!(
                                "LOOP_START node ({}) must have a non-empty loop_id in its config",
                                node.id
                            ),
                        )),
                    }
                }
                "LOOP_END" => {
                    let loop_id = node
                        .inner
                        .get("loop_id")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .filter(|s| !s.is_empty());
                    let loop_start_node_id = node
                        .inner
                        .get("loop_start_node_id")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .filter(|s| !s.is_empty());
                    match loop_id {
                        Some(id) => {
                            loop_ends.insert(node.id.clone(), (id, loop_start_node_id));
                        }
                        None => errors.push(ValidationError::new(
                            format!("nodes.{}", node.id),
                            format!(
                                "LOOP_END node ({}) must have a non-empty loop_id in its config",
                                node.id
                            ),
                        )),
                    }
                }
                _ => {}
            }
        }

        // Duplicate loop ids per node kind.
        let start_ids_by_loop: HashMap<String, Vec<String>> =
            loop_starts
                .iter()
                .fold(HashMap::new(), |mut acc, (nid, lid)| {
                    acc.entry(lid.clone()).or_default().push(nid.clone());
                    acc
                });
        let end_ids_by_loop: HashMap<String, Vec<String>> =
            loop_ends
                .iter()
                .fold(HashMap::new(), |mut acc, (nid, (lid, _))| {
                    acc.entry(lid.clone()).or_default().push(nid.clone());
                    acc
                });
        for (loop_id, node_ids) in &start_ids_by_loop {
            if node_ids.len() > 1 {
                errors.push(ValidationError::new(
                    "nodes",
                    format!(
                        "Multiple LOOP_START nodes share the same loopId ({}): [{}]",
                        loop_id,
                        node_ids.join(", ")
                    ),
                ));
            }
        }
        for (loop_id, node_ids) in &end_ids_by_loop {
            if node_ids.len() > 1 {
                errors.push(ValidationError::new(
                    "nodes",
                    format!(
                        "Multiple LOOP_END nodes share the same loopId ({}): [{}]",
                        loop_id,
                        node_ids.join(", ")
                    ),
                ));
            }
        }

        // Pairing and boundary edges.
        for (node_id, loop_id) in &loop_starts {
            let ends = end_ids_by_loop
                .get(loop_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if ends.is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node_id),
                    format!(
                        "LOOP_START node ({}) with loopId ({}) has no matching LOOP_END node",
                        node_id, loop_id
                    ),
                ));
            }
            if outgoing_edges(graph, node_id).is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node_id),
                    format!(
                        "LOOP_START node ({}) must have at least one outgoing edge",
                        node_id
                    ),
                ));
            }
            if incoming_edges(graph, node_id).is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node_id),
                    format!(
                        "LOOP_START node ({}) must have at least one incoming edge",
                        node_id
                    ),
                ));
            }
        }

        let loop_start_ids: HashSet<&str> = loop_starts.keys().map(String::as_str).collect();
        for (node_id, (loop_id, loop_start_node_id)) in &loop_ends {
            let starts = start_ids_by_loop
                .get(loop_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if starts.is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node_id),
                    format!(
                        "LOOP_END node ({}) with loopId ({}) has no matching LOOP_START node",
                        node_id, loop_id
                    ),
                ));
            }
            if outgoing_edges(graph, node_id).is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node_id),
                    format!(
                        "LOOP_END node ({}) must have at least one outgoing edge",
                        node_id
                    ),
                ));
            }

            if let Some(ref referenced) = loop_start_node_id {
                if !loop_start_ids.contains(referenced.as_str()) {
                    errors.push(ValidationError::new(
                        format!("nodes.{}", node_id),
                        format!(
                            "LOOP_END node ({}) references non-existent LOOP_START node ({}) via loop_start_node_id",
                            node_id, referenced
                        ),
                    ));
                } else if let Some(start_loop_id) = loop_starts.get(referenced) {
                    if start_loop_id != loop_id {
                        errors.push(ValidationError::new(
                            format!("nodes.{}", node_id),
                            format!(
                                "LOOP_END node ({}) loopId ({}) does not match the loopId ({}) of the referenced LOOP_START node ({})",
                                node_id, loop_id, start_loop_id, referenced
                            ),
                        ));
                    }
                }
            }
        }

        errors
    }

    /// SYNC nodes must reference an existing fork path on both sides and
    /// have well-formed variable mappings.
    fn validate_sync_nodes(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let mut fork_path_ids_set: HashSet<String> = HashSet::new();
        for node in &graph.nodes {
            if node.node_type == "FORK" {
                fork_path_ids_set.extend(fork_path_ids(node));
            }
        }

        for node in &graph.nodes {
            if node.node_type != "SYNC" {
                continue;
            }

            let source_path_id = node
                .inner
                .get("source_path_id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .filter(|s| !s.is_empty());
            let target_path_id = node
                .inner
                .get("target_path_id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .filter(|s| !s.is_empty());

            match &source_path_id {
                Some(path_id) if fork_path_ids_set.contains(path_id) => {}
                Some(path_id) => errors.push(ValidationError::new(
                    format!("nodes.{}.config.source_path_id", node.id),
                    format!(
                        "SYNC node '{}' has source_path_id '{}' that does not exist in any FORK node's fork_paths",
                        node.id, path_id
                    ),
                )),
                None => errors.push(ValidationError::new(
                    format!("nodes.{}.config.source_path_id", node.id),
                    format!("SYNC node '{}' is missing required source_path_id", node.id),
                )),
            }

            if let Some(path_id) = &target_path_id {
                if !fork_path_ids_set.contains(path_id) {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.target_path_id", node.id),
                        format!(
                            "SYNC node '{}' has target_path_id '{}' that does not exist in any FORK node's fork_paths",
                            node.id, path_id
                        ),
                    ));
                }
            }

            if let Some(mappings) = node
                .inner
                .get("variable_mappings")
                .and_then(|v| v.as_array())
            {
                for (idx, mapping) in mappings.iter().enumerate() {
                    let has_source = mapping
                        .get("source_path")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| !s.is_empty());
                    let has_internal = mapping
                        .get("internal_name")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| !s.is_empty());
                    if !has_source {
                        errors.push(ValidationError::new(
                            format!("nodes.{}.config.variable_mappings[{}]", node.id, idx),
                            format!(
                                "SYNC node '{}' has variableMapping with missing source_path",
                                node.id
                            ),
                        ));
                    }
                    if !has_internal {
                        errors.push(ValidationError::new(
                            format!("nodes.{}.config.variable_mappings[{}]", node.id, idx),
                            format!(
                                "SYNC node '{}' has variableMapping with missing internal_name",
                                node.id
                            ),
                        ));
                    }
                }
            }

            if incoming_edges(graph, &node.id).is_empty()
                && outgoing_edges(graph, &node.id).is_empty()
            {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!(
                        "SYNC node '{}' is isolated, has no incoming or outgoing edges",
                        node.id
                    ),
                ));
            }
        }

        errors
    }

    /// EMBED_GRAPH nodes must reference an embed id or carry an inline graph
    /// definition, and cannot declare variable mappings.
    fn validate_embed_graph(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for node in &graph.nodes {
            if node.node_type != "EMBED_GRAPH" {
                continue;
            }

            let has_embed_id = node
                .inner
                .get("embed_id")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty());
            let has_inline = node
                .inner
                .get("graph_definition")
                .is_some_and(|v| !v.is_null());
            if !has_embed_id && !has_inline {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!(
                        "EMBED_GRAPH node ({}) is missing embed_id configuration",
                        node.id
                    ),
                ));
            }

            let has_variable_inputs = node
                .inner
                .get("variable_inputs")
                .and_then(|v| v.as_array())
                .is_some_and(|arr| !arr.is_empty());
            let has_variable_outputs = node
                .inner
                .get("variable_outputs")
                .and_then(|v| v.as_array())
                .is_some_and(|arr| !arr.is_empty());
            if has_variable_inputs {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!(
                        "EMBED_GRAPH node '{}' should not have variable_inputs. Use SUBGRAPH for variable passing.",
                        node.id
                    ),
                ));
            }
            if has_variable_outputs {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!(
                        "EMBED_GRAPH node '{}' should not have variable_outputs. Use SUBGRAPH for variable passing.",
                        node.id
                    ),
                ));
            }
        }

        errors
    }

    /// SUBGRAPH nodes must reference a subgraph id (or embed id); the
    /// variable mapping format is checked by the node-level validators.
    fn validate_subgraph_nodes(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for node in &graph.nodes {
            if node.node_type != "SUBGRAPH" {
                continue;
            }
            let has_id = node
                .inner
                .get("subgraph_id")
                .or_else(|| node.inner.get("embed_id"))
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty());
            if !has_id {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!(
                        "SUBGRAPH node ({}) is missing subgraph_id configuration",
                        node.id
                    ),
                ));
            }
        }

        errors
    }

    /// ROUTE targets must resolve to real graph nodes: every condition
    /// target and the default target are instance-level references.
    /// Condition expressions are checked for static syntax (empty, unknown
    /// function, arity, unbalanced delimiters); value semantics stay runtime.
    /// A ROUTE must declare at least one condition or a default target, and
    /// no condition target may duplicate the default target.
    fn validate_route_targets(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        use std::collections::HashSet;
        let mut errors = Vec::new();
        let node_ids: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
        for node in &graph.nodes {
            if node.node_type != "ROUTE" {
                continue;
            }
            let conditions = node.inner.get("conditions").and_then(|v| v.as_array());
            let has_conditions = conditions.is_some_and(|c| !c.is_empty());
            let default_target = node
                .inner
                .get("default_target_node_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            if !has_conditions && default_target.is_none() {
                errors.push(ValidationError::new(
                    format!("nodes.{}.config", node.id),
                    format!(
                        "ROUTE node '{}' must define at least one condition or a default_target_node_id",
                        node.id
                    ),
                ));
            }
            if let Some(conditions) = conditions {
                for (idx, condition) in conditions.iter().enumerate() {
                    if let Some(expression) = condition.get("expression").and_then(|v| v.as_str()) {
                        if expression.trim().is_empty() {
                            errors.push(ValidationError::new(
                                format!("nodes.{}.config.conditions[{}].expression", node.id, idx),
                                format!(
                                    "ROUTE node '{}' has an empty condition expression",
                                    node.id
                                ),
                            ));
                        } else if let Err(reason) =
                            wf_core::condition::ConditionEvaluator::validate_syntax(expression)
                        {
                            errors.push(ValidationError::new(
                                format!("nodes.{}.config.conditions[{}].expression", node.id, idx),
                                format!(
                                    "ROUTE node '{}' has an invalid condition expression: {}",
                                    node.id, reason
                                ),
                            ));
                        }
                    }
                    if let Some(target) = condition
                        .get("target_node_id")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                    {
                        if !node_ids.contains(target) {
                            errors.push(ValidationError::new(
                                format!("nodes.{}.config.conditions[{}]", node.id, idx),
                                format!(
                                    "ROUTE node '{}' targets unknown node '{}'",
                                    node.id, target
                                ),
                            ));
                        }
                        if Some(target) == default_target {
                            errors.push(ValidationError::new(
                                format!("nodes.{}.config.conditions[{}]", node.id, idx),
                                format!(
                                    "ROUTE node '{}' condition target '{}' duplicates the default target; use distinct targets",
                                    node.id, target
                                ),
                            ));
                        }
                    }
                }
            }
            if let Some(target) = node
                .inner
                .get("default_target_node_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                if !node_ids.contains(target) {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.default_target_node_id", node.id),
                        format!(
                            "ROUTE node '{}' default target '{}' does not exist in the graph",
                            node.id, target
                        ),
                    ));
                }
            }
        }
        errors
    }

    /// FORK branch entry nodes must resolve to real graph nodes.
    fn validate_fork_children(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        use std::collections::HashSet;
        let mut errors = Vec::new();
        let node_ids: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
        for node in &graph.nodes {
            if node.node_type != "FORK" {
                continue;
            }
            if let Some(paths) = node.inner.get("fork_paths").and_then(|v| v.as_array()) {
                for (idx, path) in paths.iter().enumerate() {
                    if let Some(child) = path
                        .get("child_node_id")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                    {
                        if !node_ids.contains(child) {
                            errors.push(ValidationError::new(
                                format!("nodes.{}.config.fork_paths[{}]", node.id, idx),
                                format!(
                                    "FORK node '{}' branch targets unknown node '{}'",
                                    node.id, child
                                ),
                            ));
                        }
                    }
                }
            }
        }
        errors
    }

    /// Internal connectivity of triggered subgraphs: every node must be
    /// reachable from START_FROM_MESSAGE and able to reach
    /// CONTINUE_FROM_MESSAGE.
    fn validate_triggered_subgraph(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let start_id = graph
            .nodes
            .iter()
            .find(|n| n.node_type == "START_FROM_MESSAGE")
            .map(|n| n.id.clone());
        let end_id = graph
            .nodes
            .iter()
            .find(|n| n.node_type == "CONTINUE_FROM_MESSAGE")
            .map(|n| n.id.clone());

        let (Some(start_id), Some(end_id)) = (start_id, end_id) else {
            return errors;
        };

        let reachable_from_start = get_reachable_nodes(graph, &start_id);
        for node in &graph.nodes {
            if node.node_type == "START_FROM_MESSAGE" {
                continue;
            }
            if !reachable_from_start.contains(&node.id) {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!(
                        "Node '{}' is not reachable from START_FROM_MESSAGE",
                        node.id
                    ),
                ));
            }
        }

        for node in &graph.nodes {
            if node.node_type == "CONTINUE_FROM_MESSAGE" {
                continue;
            }
            if !reachable_from_start.contains(&node.id) {
                continue;
            }
            let reachable = get_reachable_nodes(graph, &node.id);
            if !reachable.contains(&end_id) {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    format!("Node '{}' cannot reach CONTINUE_FROM_MESSAGE", node.id),
                ));
            }
        }

        errors
    }

    /// Structural cycles are rejected. Loop continuation edges
    /// (LOOP_END -> LOOP_START) are legal control flow and excluded.
    fn validate_cycles(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let result = detect_cycles(graph);
        if result.has_cycle {
            vec![ValidationError::new(
                "nodes",
                format!(
                    "Circular dependencies exist in the workflow: cycle through nodes [{}]",
                    result.cycle_nodes.join(", ")
                ),
            )]
        } else {
            Vec::new()
        }
    }

    /// Reachability for normal workflows: every node must be reachable from
    /// START and must reach an END node. Triggered subgraphs use their own
    /// connectivity validation instead.
    fn validate_reachability(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let has_trigger = graph
            .nodes
            .iter()
            .any(|n| n.node_type == "START_FROM_MESSAGE" || n.node_type == "CONTINUE_FROM_MESSAGE");
        if has_trigger {
            return Vec::new();
        }

        let mut errors = Vec::new();
        let analysis = analyze_reachability(graph);

        for node_id in &analysis.unreachable_nodes {
            errors.push(ValidationError::new(
                format!("nodes.{}", node_id),
                format!("Node ({}) is not reachable from START node", node_id),
            ));
        }
        for node_id in &analysis.dead_end_nodes {
            errors.push(ValidationError::new(
                format!("nodes.{}", node_id),
                format!("Node ({}) cannot reach END node", node_id),
            ));
        }

        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::workflow::edge::EdgeType;

    fn make_node(id: &str, node_type: &str) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner: serde_json::json!({}),
        }
    }

    fn make_node_with_inner(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn make_edge(id: &str, source: &str, target: &str) -> WorkflowEdge {
        WorkflowEdge {
            id: id.to_string(),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
        }
    }

    fn make_graph(
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
            start_node_id: start.map(|s| s.to_string()),
            end_node_ids: ends.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_valid_linear_graph() {
        let graph = make_graph(
            vec![make_node("start", "START"), make_node("end", "END")],
            vec![make_edge("e1", "start", "end")],
            Some("start"),
            vec!["end"],
        );
        assert!(GraphValidator::validate(graph).is_ok());
    }

    #[test]
    fn test_empty_nodes() {
        let graph = make_graph(vec![], vec![], None, vec![]);
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("at least one node")));
    }

    #[test]
    fn test_duplicate_node_id() {
        let graph = make_graph(
            vec![make_node("n1", "START"), make_node("n1", "END")],
            vec![make_edge("e1", "n1", "n1")],
            Some("n1"),
            vec!["n1"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("Duplicate")));
    }

    #[test]
    fn test_missing_start() {
        let graph = make_graph(
            vec![make_node("n1", "VARIABLE")],
            vec![],
            Some("n1"),
            vec!["n1"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("START")));
    }

    #[test]
    fn test_missing_edge_target() {
        let graph = make_graph(
            vec![make_node("start", "START"), make_node("end", "END")],
            vec![make_edge("e1", "start", "nonexistent")],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("not found")));
    }

    #[test]
    fn test_trigger_graph() {
        let graph = make_graph(
            vec![
                make_node("message_start", "START_FROM_MESSAGE"),
                make_node("message_end", "CONTINUE_FROM_MESSAGE"),
            ],
            vec![make_edge("e1", "message_start", "message_end")],
            None,
            vec![],
        );
        assert!(GraphValidator::validate(graph).is_ok());
    }

    #[test]
    fn test_trigger_graph_with_start() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node("message_start", "START_FROM_MESSAGE"),
                make_node("message_end", "CONTINUE_FROM_MESSAGE"),
            ],
            vec![],
            None,
            vec![],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("cannot contain START")));
    }

    #[test]
    fn test_fork_with_empty_branches() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner("fork", "FORK", serde_json::json!({"fork_paths": []})),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "fork"),
                make_edge("e2", "fork", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("empty fork_paths")));
    }

    #[test]
    fn test_isolated_node_detected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node("v1", "VARIABLE"),
                make_node("end", "END"),
            ],
            vec![make_edge("e1", "start", "end")],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("isolated")));
    }

    #[test]
    fn test_start_cannot_have_incoming_edges() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node("v1", "VARIABLE"),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "v1"),
                make_edge("e2", "v1", "end"),
                make_edge("e3", "v1", "start"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("START node cannot have incoming edges")));
    }

    #[test]
    fn test_end_cannot_have_outgoing_edges() {
        let graph = make_graph(
            vec![make_node("start", "START"), make_node("end", "END")],
            vec![
                make_edge("e1", "start", "end"),
                make_edge("e2", "end", "start"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e
            .message
            .contains("END node (end) cannot have outgoing edges")));
    }

    #[test]
    fn test_cycle_detected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node("a", "VARIABLE"),
                make_node("b", "VARIABLE"),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "a"),
                make_edge("e2", "a", "b"),
                make_edge("e3", "b", "a"),
                make_edge("e4", "b", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("Circular dependencies")));
    }

    #[test]
    fn test_loop_pair_is_valid() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner("ls", "LOOP_START", serde_json::json!({"loop_id": "l1"})),
                make_node_with_inner(
                    "body",
                    "VARIABLE",
                    serde_json::json!({"variable_name": "body", "expression": "1"}),
                ),
                make_node_with_inner(
                    "le",
                    "LOOP_END",
                    serde_json::json!({"loop_id": "l1", "loop_start_node_id": "ls"}),
                ),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "ls"),
                make_edge("e2", "ls", "body"),
                make_edge("e3", "body", "le"),
                make_edge("e4", "le", "ls"),
                make_edge("e5", "le", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        assert!(GraphValidator::validate(graph).is_ok());
    }

    #[test]
    fn test_unpaired_loop_end_detected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner("ls", "LOOP_START", serde_json::json!({"loop_id": "l1"})),
                make_node("end", "END"),
            ],
            vec![make_edge("e1", "start", "ls"), make_edge("e2", "ls", "end")],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("no matching LOOP_END")));
    }

    #[test]
    fn test_fork_join_pairing_ok() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "fork",
                    "FORK",
                    serde_json::json!({
                        "fork_paths": [
                            {"path_id": "p1", "child_node_id": "a1"},
                            {"path_id": "p2", "child_node_id": "a2"}
                        ]
                    }),
                ),
                make_node_with_inner(
                    "a1",
                    "VARIABLE",
                    serde_json::json!({"variable_name": "a1", "expression": "1"}),
                ),
                make_node_with_inner(
                    "a2",
                    "VARIABLE",
                    serde_json::json!({"variable_name": "a2", "expression": "2"}),
                ),
                make_node_with_inner(
                    "join",
                    "JOIN",
                    serde_json::json!({"fork_path_ids": ["p1", "p2"]}),
                ),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "fork"),
                make_edge("e2", "fork", "a1"),
                make_edge("e3", "fork", "a2"),
                make_edge("e4", "a1", "join"),
                make_edge("e5", "a2", "join"),
                make_edge("e6", "join", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        assert!(GraphValidator::validate(graph).is_ok());
    }

    #[test]
    fn test_fork_without_join_rejected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "fork",
                    "FORK",
                    serde_json::json!({
                        "fork_paths": [{"path_id": "p1", "child_node_id": "a1"}]
                    }),
                ),
                make_node("a1", "VARIABLE"),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "fork"),
                make_edge("e2", "fork", "a1"),
                make_edge("e3", "a1", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e
            .message
            .contains("FORK node (fork) has no matching JOIN node")));
    }

    #[test]
    fn test_fork_join_path_mismatch_rejected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "fork",
                    "FORK",
                    serde_json::json!({
                        "fork_paths": [
                            {"path_id": "p1", "child_node_id": "a1"},
                            {"path_id": "p2", "child_node_id": "a2"}
                        ]
                    }),
                ),
                make_node("a1", "VARIABLE"),
                make_node("a2", "VARIABLE"),
                make_node_with_inner(
                    "join",
                    "JOIN",
                    serde_json::json!({"fork_path_ids": ["p1", "p3"]}),
                ),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "fork"),
                make_edge("e2", "fork", "a1"),
                make_edge("e3", "fork", "a2"),
                make_edge("e4", "a1", "join"),
                make_edge("e5", "a2", "join"),
                make_edge("e6", "join", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("do not match")));
    }

    #[test]
    fn test_fork_join_unreachable_rejected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "fork",
                    "FORK",
                    serde_json::json!({
                        "fork_paths": [{"path_id": "p1", "child_node_id": "a1"}]
                    }),
                ),
                make_node("a1", "VARIABLE"),
                make_node_with_inner("join", "JOIN", serde_json::json!({"fork_path_ids": ["p1"]})),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "fork"),
                make_edge("e2", "fork", "a1"),
                // a1 never connects to join: join is fed by nothing.
                make_edge("e3", "join", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("cannot reach the paired JOIN node")));
    }

    #[test]
    fn test_sync_with_invalid_path_rejected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "fork",
                    "FORK",
                    serde_json::json!({
                        "fork_paths": [{"path_id": "p1", "child_node_id": "a1"}]
                    }),
                ),
                make_node("a1", "VARIABLE"),
                make_node_with_inner("sync", "SYNC", serde_json::json!({"source_path_id": "pX"})),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "fork"),
                make_edge("e2", "fork", "a1"),
                make_edge("e3", "a1", "sync"),
                make_edge("e4", "sync", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e
            .message
            .contains("does not exist in any FORK node's fork_paths")));
    }

    #[test]
    fn test_embed_graph_requires_definition() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node("embed", "EMBED_GRAPH"),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "embed"),
                make_edge("e2", "embed", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("missing embed_id configuration")));
    }

    #[test]
    fn test_embed_graph_with_inline_definition_passes_config() {
        let inline = make_graph(
            vec![make_node("s2", "START"), make_node("e2", "END")],
            vec![make_edge("e2-1", "s2", "e2")],
            Some("s2"),
            vec!["e2"],
        );
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "embed",
                    "EMBED_GRAPH",
                    serde_json::json!({"graph_definition": inline}),
                ),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "embed"),
                make_edge("e2", "embed", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        assert!(
            GraphValidator::validate(graph).is_ok(),
            "inline graph definition must satisfy the embed requirement"
        );
    }

    #[test]
    fn test_triggered_subgraph_disconnected_detected() {
        let graph = make_graph(
            vec![
                make_node("ts", "START_FROM_MESSAGE"),
                make_node("v1", "VARIABLE"),
                make_node("te", "CONTINUE_FROM_MESSAGE"),
            ],
            vec![make_edge("e1", "ts", "te")],
            None,
            vec![],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("not reachable from START_FROM_MESSAGE")));
    }

    #[test]
    fn test_unreachable_node_detected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node("a", "VARIABLE"),
                make_node("end", "END"),
            ],
            vec![make_edge("e1", "start", "end")],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("is not reachable from START")));
    }

    #[test]
    fn test_llm_node_missing_profile_rejected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node("llm", "LLM"),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "llm"),
                make_edge("e2", "llm", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("missing required config 'profile_id'")));
    }

    #[test]
    fn test_inconsistent_tool_call_formats_rejected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "l1",
                    "LLM",
                    serde_json::json!({
                        "profile_id": "mock",
                        "tool_call_format": "native",
                    }),
                ),
                make_node_with_inner(
                    "l2",
                    "LLM",
                    serde_json::json!({
                        "profile_id": "mock",
                        "tool_call_format": "xml",
                    }),
                ),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "l1"),
                make_edge("e2", "l1", "l2"),
                make_edge("e3", "l2", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("Inconsistent tool call protocols")));
    }

    #[test]
    fn test_route_with_invalid_expression_syntax_rejected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "route",
                    "ROUTE",
                    serde_json::json!({
                        "conditions": [{"expression": "eq(only_one)", "target_node_id": "a"}],
                        "default_target_node_id": "end",
                    }),
                ),
                make_node_with_inner(
                    "a",
                    "VARIABLE",
                    serde_json::json!({"variable_name": "x", "expression": "1"}),
                ),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "route"),
                make_edge("e2", "route", "a"),
                make_edge("e3", "route", "end"),
                make_edge("e4", "a", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("invalid condition expression")));
    }

    #[test]
    fn test_route_without_conditions_or_default_rejected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner("route", "ROUTE", serde_json::json!({})),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "route"),
                make_edge("e2", "route", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e
            .message
            .contains("at least one condition or a default_target_node_id")));
    }

    #[test]
    fn test_route_duplicate_default_target_rejected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "route",
                    "ROUTE",
                    serde_json::json!({
                        "conditions": [{"expression": "eq(a, 1)", "target_node_id": "end"}],
                        "default_target_node_id": "end",
                    }),
                ),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "route"),
                make_edge("e2", "route", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("duplicates the default target")));
    }

    #[test]
    fn test_route_with_default_only_passes() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "route",
                    "ROUTE",
                    serde_json::json!({"default_target_node_id": "end"}),
                ),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "route"),
                make_edge("e2", "route", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        assert!(GraphValidator::validate(graph).is_ok());
    }

    #[test]
    fn test_fork_with_empty_path_id_rejected() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "fork",
                    "FORK",
                    serde_json::json!({
                        "fork_paths": [{"path_id": "", "child_node_id": "a1"}]
                    }),
                ),
                make_node_with_inner(
                    "a1",
                    "VARIABLE",
                    serde_json::json!({"variable_name": "x", "expression": "1"}),
                ),
                make_node_with_inner("join", "JOIN", serde_json::json!({"fork_path_ids": ["p1"]})),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "fork"),
                make_edge("e2", "fork", "a1"),
                make_edge("e3", "a1", "join"),
                make_edge("e4", "join", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("empty path_id")));
    }

    #[test]
    fn test_fork_join_count_mismatch_reports_counts() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "fork",
                    "FORK",
                    serde_json::json!({
                        "fork_paths": [
                            {"path_id": "p1", "child_node_id": "a1"},
                            {"path_id": "p2", "child_node_id": "a2"}
                        ]
                    }),
                ),
                make_node_with_inner(
                    "a1",
                    "VARIABLE",
                    serde_json::json!({"variable_name": "x", "expression": "1"}),
                ),
                make_node_with_inner(
                    "a2",
                    "VARIABLE",
                    serde_json::json!({"variable_name": "y", "expression": "2"}),
                ),
                make_node_with_inner("join", "JOIN", serde_json::json!({"fork_path_ids": ["p1"]})),
                make_node("end", "END"),
            ],
            vec![
                make_edge("e1", "start", "fork"),
                make_edge("e2", "fork", "a1"),
                make_edge("e3", "fork", "a2"),
                make_edge("e4", "a1", "join"),
                make_edge("e5", "a2", "join"),
                make_edge("e6", "join", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("do not match") && e.message.contains("2 path(s)")));
    }

    #[test]
    fn test_format_validation_report_lists_each_finding() {
        let errors = vec![
            ValidationError::new("nodes.a", "first problem"),
            ValidationError::new("nodes.b.config.x", "second problem"),
        ];
        let report = format_validation_report(&errors);
        assert!(report.contains("2 error(s) found:"));
        assert!(report.contains("[nodes.a] first problem"));
        assert!(report.contains("[nodes.b.config.x] second problem"));

        let empty = format_validation_report(&[]);
        assert!(empty.contains("0 error(s) found:"));
    }
}
