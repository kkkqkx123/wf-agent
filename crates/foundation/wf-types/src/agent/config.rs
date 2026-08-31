use serde::{Deserialize, Serialize};

use super::AgentHookConfig;
use crate::checkpoint::agent::AgentCheckpointConfig;
use crate::dynamic_context::DynamicContextConfig;
use crate::llm::request::ToolCallProtocolViolationPolicy;
use crate::message::Message;
use crate::tool::AvailableTools;
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
    /// Wall-clock execution budget for the agent loop in milliseconds; `0`
    /// disables the limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_execution_time: Option<u64>,
    /// Maximum LLM call retries on transient failures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    /// Single LLM call timeout in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_timeout: Option<u64>,
    /// Max duration the loop may stay paused before it is stopped in
    /// milliseconds; `0` disables the limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_pause_duration: Option<u64>,
    /// Cumulative token limit for the conversation; `0` disables limit
    /// checks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_limit: Option<u64>,
    /// Warning threshold percentage of the token limit (default 80).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_warning_threshold: Option<u32>,
    /// Token usage tracking switch (enabled by default unless disabled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_token_tracking: Option<bool>,
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
    pub dynamic_context: Option<DynamicContextConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<AgentCheckpointConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation_policy: Option<ToolCallProtocolViolationPolicy>,
}
