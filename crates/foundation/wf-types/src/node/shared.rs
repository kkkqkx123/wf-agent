use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeIdentity {
    pub id: super::super::Id,
    pub name: Option<String>,
    pub node_type: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NodeExecutionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_before_execute: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_after_execute: Option<bool>,
    /// Structured per-node checkpoint configuration (timing, cadence,
    /// description). Merged into the node config at graph conversion time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<crate::checkpoint::NodeCheckpointConfig>,
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

impl NodeExecutionConfig {
    /// Flatten the execution config into the node's JSON config blob
    /// (`camelCase` keys, `None` fields omitted). Used at graph conversion
    /// time so the runtime can read execution behavior from the node's
    /// `inner` config without a separate typed channel.
    pub fn config_fields(&self) -> serde_json::Map<String, serde_json::Value> {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }
}
