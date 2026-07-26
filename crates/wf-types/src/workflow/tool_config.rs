use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AvailableTools {
    pub available: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_workflows: Option<Vec<String>>,
}
