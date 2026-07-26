use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<crate::checkpoint::workflow::WorkflowCheckpointConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<super::super::execution::RetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_approval: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<crate::tool::AvailableTools>,
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
