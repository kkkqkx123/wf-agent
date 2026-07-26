use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalOptions {
    pub require_approval: bool,
    pub risk_threshold: Option<String>,
    pub auto_approve_patterns: Option<Vec<String>>,
}
