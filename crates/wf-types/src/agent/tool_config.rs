use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentToolConfig {
    pub tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_workflows: Option<Vec<String>>,
}
