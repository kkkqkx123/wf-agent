use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeIdentity {
    pub id: crate::Id,
    #[serde(rename = "type")]
    pub node_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StaticNodeDisplayProps {
    pub name: String,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnFailure {
    Fail,
    Retry,
    Continue,
}

impl Default for OnFailure {
    fn default() -> Self {
        Self::Fail
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NodeExecutionConfig {
    pub hooks: Option<Vec<crate::hook::BaseHookConfig>>,
    pub checkpoint_before_execute: Option<bool>,
    pub checkpoint_after_execute: Option<bool>,
    pub output_id: Option<String>,
    #[serde(default)]
    pub on_failure: OnFailure,
    pub max_retries: Option<u32>,
    pub retry_delay_ms: Option<i64>,
    pub exponential_backward: Option<bool>,
    pub fallback_output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaseRuntimeNode {
    #[serde(flatten)]
    pub identity: NodeIdentity,
    #[serde(flatten)]
    pub execution_config: NodeExecutionConfig,
    #[serde(flatten)]
    pub runtime_context: super::context::RuntimeNodeContext,
}
