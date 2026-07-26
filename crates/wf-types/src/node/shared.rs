use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeIdentity {
    pub id: super::super::Id,
    pub name: Option<String>,
    pub node_type: String,
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
    pub retry_policy: Option<crate::execution::RetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<crate::execution::FailureAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_id: Option<String>,
}
