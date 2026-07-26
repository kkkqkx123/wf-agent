use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_before_node: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_after_node: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_checkpoints: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_config: Option<CheckpointConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<super::super::execution::RetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_approval: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<super::AvailableTools>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_messages: Option<Vec<super::super::message::Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_template_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_template_variables: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub static_contexts: Option<Vec<serde_json::Value>>,
}
