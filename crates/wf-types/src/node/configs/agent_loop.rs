use serde::{Deserialize, Serialize};

use crate::agent::AgentDefinition;
use crate::execution::FailureAction;
use crate::execution::RetryPolicy;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopNodeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_definition: Option<AgentDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<FailureAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopNodeOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_response: Option<String>,
    pub tool_call_count: u32,
    pub iteration_count: u32,
}
