use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InlineAgentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_delay: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exponential_backoff: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopNodeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_config: Option<InlineAgentConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopNodeOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_response: Option<String>,
    pub tool_call_count: u32,
    pub iteration_count: u32,
}
