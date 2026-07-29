use std::collections::{HashMap, HashSet};

use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self { field: field.into(), message: message.into() }
    }
}

pub type ValidationResult = Result<(), Vec<ValidationError>>;

pub struct GraphValidator;

impl GraphValidator {
    pub fn validate(graph: &WorkflowGraphStructure) -> ValidationResult {
        let mut errors: Vec<ValidationError> = Vec::new();
        errors.extend(Self::validate_nodes(graph));
        errors.extend(Self::validate_edges(graph));
        errors.extend(Self::validate_start_end(graph));
        errors.extend(Self::validate_references(graph));
        errors.extend(Self::validate_fork_join(graph));

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    fn validate_nodes(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let mut node_ids = HashSet::new();

        if graph.nodes.is_empty() {
            errors.push(ValidationError::new("nodes", "Graph must have at least one node"));
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

        let node_types: HashMap<&str, &str> = graph.nodes.iter()
            .map(|n| (n.id.as_str(), n.node_type.as_str()))
            .collect();

        let start_count = node_types.values().filter(|t| **t == "START").count();
        let end_count = node_types.values().filter(|t| **t == "END").count();
        let trigger_start_count = node_types.values().filter(|t| **t == "START_FROM_TRIGGER").count();
        let trigger_end_count = node_types.values().filter(|t| **t == "CONTINUE_FROM_TRIGGER").count();

        let has_special = trigger_start_count > 0 || trigger_end_count > 0;

        if has_special {
            if trigger_start_count != 1 {
                errors.push(ValidationError::new(
                    "nodes",
                    "Triggered subgraph must have exactly one START_FROM_TRIGGER node",
                ));
            }
            if trigger_end_count != 1 {
                errors.push(ValidationError::new(
                    "nodes",
                    "Triggered subgraph must have exactly one CONTINUE_FROM_TRIGGER node",
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
                errors.push(ValidationError::new("nodes", "Workflow must have a START node"));
            } else if start_count > 1 {
                errors.push(ValidationError::new("nodes", "Workflow must have exactly one START node"));
            }

            if end_count == 0 {
                errors.push(ValidationError::new("nodes", "Workflow must have at least one END node"));
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
            errors.push(ValidationError::new("start_node_id", "Graph must have a start_node_id"));
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

    fn validate_fork_join(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let fork_nodes: Vec<&WorkflowNode> = graph.nodes.iter()
            .filter(|n| n.node_type == "FORK")
            .collect();

        for fork in &fork_nodes {
            let outgoing: Vec<&WorkflowEdge> = graph.edges.iter()
                .filter(|e| e.source_node_id == fork.id)
                .collect();

            if outgoing.is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", fork.id),
                    format!("FORK node '{}' has no outgoing edges", fork.id),
                ));
            }

            if let Some(branches) = fork.inner.get("branches").and_then(|b| b.as_array()) {
                if branches.is_empty() {
                    errors.push(ValidationError::new(
                        format!("nodes.{}", fork.id),
                        format!("FORK node '{}' has empty branches", fork.id),
                    ));
                }
            }
        }

        let join_nodes: Vec<&WorkflowNode> = graph.nodes.iter()
            .filter(|n| n.node_type == "JOIN")
            .collect();

        for join in &join_nodes {
            let incoming: Vec<&WorkflowEdge> = graph.edges.iter()
                .filter(|e| e.target_node_id == join.id)
                .collect();

            if incoming.is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes.{}", join.id),
                    format!("JOIN node '{}' has no incoming edges", join.id),
                ));
            }
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

    fn make_graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>, start: Option<&str>, ends: Vec<&str>) -> WorkflowGraphStructure {
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
        assert!(GraphValidator::validate(&graph).is_ok());
    }

    #[test]
    fn test_empty_nodes() {
        let graph = make_graph(vec![], vec![], None, vec![]);
        let result = GraphValidator::validate(&graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("at least one node")));
    }

    #[test]
    fn test_duplicate_node_id() {
        let graph = make_graph(
            vec![make_node("n1", "START"), make_node("n1", "END")],
            vec![make_edge("e1", "n1", "n1")],
            Some("n1"),
            vec!["n1"],
        );
        let result = GraphValidator::validate(&graph);
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
        let result = GraphValidator::validate(&graph);
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
        let result = GraphValidator::validate(&graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("not found")));
    }

    #[test]
    fn test_trigger_graph() {
        let graph = make_graph(
            vec![make_node("trigger_start", "START_FROM_TRIGGER"), make_node("trigger_end", "CONTINUE_FROM_TRIGGER")],
            vec![make_edge("e1", "trigger_start", "trigger_end")],
            None,
            vec![],
        );
        assert!(GraphValidator::validate(&graph).is_ok());
    }

    #[test]
    fn test_trigger_graph_with_start() {
        let graph = make_graph(
            vec![make_node("start", "START"), make_node("trigger_start", "START_FROM_TRIGGER"), make_node("trigger_end", "CONTINUE_FROM_TRIGGER")],
            vec![],
            None,
            vec![],
        );
        let result = GraphValidator::validate(&graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("cannot contain START")));
    }

    #[test]
    fn test_fork_with_empty_branches() {
        let graph = make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner("fork", "FORK", serde_json::json!({"branches": []})),
                make_node("end", "END"),
            ],
            vec![make_edge("e1", "start", "fork"), make_edge("e2", "fork", "end")],
            Some("start"),
            vec!["end"],
        );
        let result = GraphValidator::validate(&graph);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("empty branches")));
    }
}
