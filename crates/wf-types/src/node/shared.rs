use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeIdentity {
    pub id: super::super::Id,
    pub name: Option<String>,
    pub node_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OnFailureAction {
    Fail,
    Retry,
    Continue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeExecutionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<super::NodeHook>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_before_execute: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_after_execute: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_delay_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exponential_backoff: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<OnFailureAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_id: Option<String>,
}

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
