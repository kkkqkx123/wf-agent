use serde::{Deserialize, Serialize};

use crate::agent::AgentDefinition;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopNodeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_definition: Option<AgentDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopNodeOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_response: Option<String>,
    pub tool_call_count: u32,
    pub iteration_count: u32,
}
