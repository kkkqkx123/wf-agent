use std::collections::HashMap;

use wf_types::{ErrorCause, ErrorType, RecoveryAction};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErrorPattern {
    pub total_errors: usize,
    pub type_distribution: HashMap<String, usize>,
    pub affected_nodes: Vec<String>,
    pub most_common_type: Option<ErrorType>,
    pub has_recoverable: bool,
    pub recovery_action_count: HashMap<String, usize>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErrorMetadata {
    pub error_type: Option<ErrorType>,
    pub caused_by: Option<ErrorCause>,
    pub is_recoverable: bool,
    pub recovery_action: Option<RecoveryAction>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorRecord {
    pub id: String,
    pub execution_id: String,
    pub error: String,
    pub error_type: Option<ErrorType>,
    pub timestamp: i64,
    pub node_id: Option<String>,
    pub parent_error_id: Option<String>,
    pub error_chain: Vec<String>,
    pub root_cause_id: String,
    pub caused_by: Option<ErrorCause>,
    pub is_recoverable: bool,
    pub recovery_action: Option<RecoveryAction>,
}
