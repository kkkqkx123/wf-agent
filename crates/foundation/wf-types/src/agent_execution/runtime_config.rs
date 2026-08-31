use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::agent_execution::hooks::AgentHook;
use crate::execution::FailureAction;
use crate::llm::tool_call_format::ToolCallFormatConfig;
use crate::message::Message;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRuntimeConfig {
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
    pub available_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discoverable_tool_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_tool_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_format: Option<ToolCallFormatConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<FailureAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<AgentHook>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_context_config: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_config: Option<HashMap<String, serde_json::Value>>,
}
