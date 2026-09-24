use serde::{Deserialize, Serialize};

use crate::workflow::error_branch::ErrorRouteConfig;
use crate::Id;
use crate::Metadata;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EdgeType {
    Default,
    Conditional,
    /// Failure route: source is the node whose terminal failure triggers the
    /// jump, target is the error-branch entry node. Error edges are never
    /// part of normal control or data flow; the coordinator resolves them
    /// only from the terminal-failure routing table.
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Edge {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
    /// Failure-route config; only valid when `r#type` is `Error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_route: Option<ErrorRouteConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<EdgeMetadata>,
}
