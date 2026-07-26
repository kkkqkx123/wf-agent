use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Edge {
    pub id: super::super::Id,
    pub source_node_id: String,
    pub target_node_id: String,
    pub condition: Option<String>,
    pub label: Option<String>,
}
