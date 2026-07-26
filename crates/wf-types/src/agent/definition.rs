use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopDefinition {
    pub id: super::super::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<super::super::Version>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_template_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_template_variables: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_messages: Option<Vec<super::super::message::Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<super::super::tool::AvailableTools>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<super::AgentHookStatic>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<crate::trigger::TriggerDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_context: Option<super::super::dynamic_context::DynamicContextConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<super::super::checkpoint::agent::AgentLoopCheckpointConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation_policy: Option<super::super::llm::request::ToolCallProtocolViolationPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<super::super::Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<super::super::Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTemplate {
    pub id: super::super::Id,
    pub name: String,
    pub description: String,
    pub definition: AgentLoopDefinition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_public: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}
