use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamicPromptContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<super::super::Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u32>,
    pub current_iteration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::super::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamicPromptInjection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub static_system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_user_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopRuntimeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_execution_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_messages: Option<Vec<super::super::message::Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_format: Option<super::super::llm::tool_call_format::ToolCallFormatConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<super::hooks::AgentHook>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<super::triggers::AgentTrigger>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_context_config: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_config: Option<HashMap<String, serde_json::Value>>,
}
