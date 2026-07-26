use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowGraphStructure {
    pub node_ids: Vec<String>,
    pub edges: Vec<super::super::workflow::Edge>,
    pub entry_node_id: Option<String>,
    pub exit_node_ids: Vec<String>,
    pub adjacency_list: HashMap<String, Vec<String>>,
}
