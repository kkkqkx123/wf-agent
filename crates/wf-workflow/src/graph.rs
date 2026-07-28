
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::error::WorkflowResult;

pub struct GraphTraversal {
    graph: WorkflowGraphStructure,
}

impl GraphTraversal {
    pub fn new(graph: WorkflowGraphStructure) -> WorkflowResult<Self> {
        Ok(Self { graph })
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

    pub fn get_outgoing_edges(&self, node_id: &str) -> Vec<&wf_types::workflow_execution::WorkflowEdge> {
        self.graph.edges.iter()
            .filter(|e| e.source_node_id == node_id)
            .collect()
    }

    pub fn get_incoming_edges(&self, node_id: &str) -> Vec<&wf_types::workflow_execution::WorkflowEdge> {
        self.graph.edges.iter()
            .filter(|e| e.target_node_id == node_id)
            .collect()
    }

    pub fn is_end_node(&self, node_id: &str) -> bool {
        self.graph.end_node_ids.iter().any(|id| id == node_id)
    }

    pub fn find_ready_nodes(&self, completed: &[String]) -> Vec<String> {
        let mut ready = Vec::new();
        for node in &self.graph.nodes {
            if completed.contains(&node.id) {
                continue;
            }
            let all_deps_completed = self.graph.edges.iter()
                .filter(|e| e.target_node_id == node.id)
                .all(|e| completed.contains(&e.source_node_id));
            if all_deps_completed {
                ready.push(node.id.clone());
            }
        }
        ready
    }
}
