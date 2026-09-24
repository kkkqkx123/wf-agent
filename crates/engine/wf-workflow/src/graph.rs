use std::collections::HashSet;

use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::error::{WorkflowError, WorkflowResult};
use crate::error_branch::{is_error_edge, ErrorRouteTable};

pub struct GraphTraversal {
    graph: WorkflowGraphStructure,
    /// Error-routing table compiled once here so the terminal-failure hot path
    /// resolves jumps by lookup instead of re-parsing node config per failure.
    error_table: ErrorRouteTable,
}

impl GraphTraversal {
    pub fn graph(&self) -> &WorkflowGraphStructure {
        &self.graph
    }

    pub fn error_table(&self) -> &ErrorRouteTable {
        &self.error_table
    }

    pub fn new(graph: WorkflowGraphStructure) -> WorkflowResult<Self> {
        let node_id_set: HashSet<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();

        if node_id_set.is_empty() {
            return Err(WorkflowError::GraphError("Graph has no nodes".to_string()));
        }

        if let Some(ref start_id) = graph.start_node_id {
            if !node_id_set.contains(start_id) {
                return Err(WorkflowError::GraphError(format!(
                    "Start node '{}' not found in graph nodes",
                    start_id
                )));
            }
        }

        for edge in &graph.edges {
            if !node_id_set.contains(&edge.source_node_id) {
                return Err(WorkflowError::GraphError(format!(
                    "Edge source node '{}' not found in graph",
                    edge.source_node_id
                )));
            }
            if !node_id_set.contains(&edge.target_node_id) {
                return Err(WorkflowError::GraphError(format!(
                    "Edge target node '{}' not found in graph",
                    edge.target_node_id
                )));
            }
        }

        // Compile the error-routing table from the graph's ERROR edges, then
        // check reachability over all edges: dedicated error handlers entered
        // only through an error jump still count as reachable parts of the
        // execution graph.
        let error_table = ErrorRouteTable::build(&graph);

        if graph.start_node_id.is_some() {
            let reachable = reachable_from(&graph, graph.start_node_id.as_deref());
            for node in &graph.nodes {
                if !reachable.contains(&node.id) {
                    return Err(WorkflowError::GraphError(format!(
                        "Node '{}' is unreachable from start node",
                        node.id
                    )));
                }
            }
        }

        Ok(Self { graph, error_table })
    }

    pub fn start_node_id(&self) -> Option<&str> {
        self.graph.start_node_id.as_deref()
    }

    pub fn end_node_ids(&self) -> &[String] {
        &self.graph.end_node_ids
    }

    pub fn get_node(&self, node_id: &str) -> Option<&wf_types::workflow_execution::WorkflowNode> {
        self.graph.nodes.iter().find(|n| n.id == node_id)
    }

    pub fn get_outgoing_edges(
        &self,
        node_id: &str,
    ) -> Vec<&wf_types::workflow_execution::WorkflowEdge> {
        self.graph
            .edges
            .iter()
            .filter(|e| e.source_node_id == node_id && !is_error_edge(e))
            .collect()
    }

    pub fn get_incoming_edges(
        &self,
        node_id: &str,
    ) -> Vec<&wf_types::workflow_execution::WorkflowEdge> {
        self.graph
            .edges
            .iter()
            .filter(|e| e.target_node_id == node_id && !is_error_edge(e))
            .collect()
    }

    pub fn is_end_node(&self, node_id: &str) -> bool {
        self.graph.end_node_ids.iter().any(|id| id == node_id)
    }

    pub fn node_count(&self) -> usize {
        self.graph.nodes.len()
    }

    pub fn find_ready_nodes(&self, completed: &[String]) -> Vec<String> {
        let mut ready = Vec::new();
        for node in &self.graph.nodes {
            if completed.contains(&node.id) {
                continue;
            }
            let all_deps_completed = self
                .graph
                .edges
                .iter()
                .filter(|e| e.target_node_id == node.id && !is_error_edge(e))
                .all(|e| completed.contains(&e.source_node_id));
            if all_deps_completed {
                ready.push(node.id.clone());
            }
        }
        ready
    }
}

/// Reachability closure over all edges (normal and ERROR): a dedicated error
/// handler entered only through an error jump is still part of the execution
/// graph. The workflow catch-all default can fire from any reachable failure,
/// so its target joins the closure as soon as one node is reachable.
fn reachable_from(graph: &WorkflowGraphStructure, start: Option<&str>) -> HashSet<String> {
    let mut reachable = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    if let Some(start_id) = start {
        reachable.insert(start_id.to_string());
        queue.push_back(start_id.to_string());
    }
    let default_target = graph
        .error_default
        .as_ref()
        .map(|d| d.target_node_id.clone());
    while let Some(current) = queue.pop_front() {
        for edge in &graph.edges {
            if edge.source_node_id == current && reachable.insert(edge.target_node_id.clone()) {
                queue.push_back(edge.target_node_id.clone());
            }
        }
        if let Some(ref target) = default_target {
            if reachable.insert(target.clone()) {
                queue.push_back(target.clone());
            }
        }
    }
    reachable
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wf_types::workflow::edge::EdgeType;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowNode};

    #[test]
    fn test_graph_empty_nodes() {
        let graph = WorkflowGraphStructure {
            nodes: vec![],
            edges: vec![],
            adjacency_list: std::collections::HashMap::new(),
            reverse_adjacency_list: std::collections::HashMap::new(),
            start_node_id: None,
            end_node_ids: vec![],
            error_default: None,
        };
        let result = GraphTraversal::new(graph);
        match result {
            Err(e) => assert!(e.to_string().contains("no nodes")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_graph_missing_start_node() {
        let graph = WorkflowGraphStructure {
            nodes: vec![WorkflowNode {
                id: "node_1".to_string(),
                name: None,
                node_type: "VARIABLE".to_string(),
                inner: serde_json::json!({}),
            }],
            edges: vec![],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("nonexistent".to_string()),
            end_node_ids: vec!["node_1".to_string()],
            error_default: None,
        };
        assert!(GraphTraversal::new(graph).is_err());
    }

    #[test]
    fn test_graph_valid() {
        let graph = WorkflowGraphStructure {
            nodes: vec![
                WorkflowNode {
                    id: "start".to_string(),
                    name: None,
                    node_type: "START".to_string(),
                    inner: serde_json::json!({}),
                },
                WorkflowNode {
                    id: "end".to_string(),
                    name: None,
                    node_type: "END".to_string(),
                    inner: serde_json::json!({}),
                },
            ],
            edges: vec![WorkflowEdge {
                id: "e1".to_string(),
                source_node_id: "start".to_string(),
                target_node_id: "end".to_string(),
                r#type: EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                error_route: None,
            }],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
            error_default: None,
        };
        let traversal = GraphTraversal::new(graph).unwrap();
        assert_eq!(traversal.start_node_id(), Some("start"));
        assert_eq!(traversal.node_count(), 2);
        assert!(traversal.is_end_node("end"));
    }
}
