use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopDefinition {
    pub id: super::super::Id,
    pub name: String,
    pub description: Option<String>,
    pub max_iterations: Option<u32>,
    pub system_prompt: Option<String>,
    pub llm_profile_id: Option<String>,
    pub tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTemplate {
    pub id: super::super::Id,
    pub name: String,
    pub description: String,
    pub definition: AgentLoopDefinition,
}
