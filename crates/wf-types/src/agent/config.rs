use serde::{Deserialize, Serialize};

use super::AgentHookConfig;
use crate::checkpoint::agent::AgentCheckpointConfig;
use crate::dynamic_context::DynamicContextConfig;
use crate::llm::request::ToolCallProtocolViolationPolicy;
use crate::message::Message;
use crate::tool::AvailableTools;
use crate::trigger::TriggerDefinition;
use crate::Metadata;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_template_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_template_variables: Option<Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_messages: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<AvailableTools>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<AgentHookConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<TriggerDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_context: Option<DynamicContextConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<AgentCheckpointConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation_policy: Option<ToolCallProtocolViolationPolicy>,
}
