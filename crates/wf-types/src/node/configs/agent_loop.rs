use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopNodeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_definition: Option<crate::agent::AgentLoopDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<crate::execution::RetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<crate::execution::FailureAction>,
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
