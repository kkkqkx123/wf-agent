use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::workflow::error_branch::{ErrorRouteConfig, WorkflowErrorDefault};
use crate::workflow::EdgeType;
use crate::Id;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowNode {
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub node_type: String,
    #[serde(flatten)]
    pub inner: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowEdge {
    pub id: Id,
    pub source_node_id: String,
    pub target_node_id: String,
    pub r#type: EdgeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Failure-route config, only valid when `r#type` is `Error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_route: Option<ErrorRouteConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowGraphStructure {
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    pub adjacency_list: HashMap<String, Vec<String>>,
    pub reverse_adjacency_list: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_node_id: Option<String>,
    pub end_node_ids: Vec<String>,
    /// Workflow-level catch-all error route (from the workflow config).
    /// Applied when no node-level error edge matches a terminal failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_default: Option<WorkflowErrorDefault>,
}
