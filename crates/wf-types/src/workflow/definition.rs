use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowTemplate {
    pub id: super::super::Id,
    pub name: String,
    pub description: String,
    pub version: Option<super::super::Version>,
    pub nodes: Vec<super::super::node::BaseStaticNode>,
    pub edges: Vec<super::Edge>,
    pub config: Option<super::WorkflowConfig>,
    pub metadata: Option<super::super::Metadata>,
}
