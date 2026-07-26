use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeNodeContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_metadata: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_node: Option<serde_json::Value>,
    pub workflow_id: crate::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_workflow_id: Option<crate::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outgoing_edge_ids: Option<Vec<crate::Id>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incoming_edge_ids: Option<Vec<crate::Id>>,
}
