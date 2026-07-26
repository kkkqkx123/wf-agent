use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowCheckpointContentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_node_results: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_variables: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_messages: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_graph: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_execution_context: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowCheckpointConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_nodes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_completion: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<WorkflowCheckpointContentConfig>,
}
