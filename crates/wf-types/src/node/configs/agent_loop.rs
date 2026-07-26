use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopNodeConfig {
    pub agent_definition_id: String,
    pub max_iterations: Option<u32>,
    pub system_prompt: Option<String>,
    pub tools: Option<Vec<String>>,
}
