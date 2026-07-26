use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpApprovalSettings {
    pub require_approval: bool,
    pub allowed_tools: Option<Vec<String>>,
    pub max_cost: Option<f64>,
}
