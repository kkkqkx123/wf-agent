use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TimeoutConfig {
    pub workflow_execution_completion: Option<i64>,
    pub workflow_execution_pause: Option<i64>,
    pub workflow_execution_cancel: Option<i64>,
    pub workflow_execution_resume: Option<i64>,
    pub child_execution_wait: Option<i64>,
    pub cascade_cancel: Option<i64>,
    pub node_completion: Option<i64>,
    pub node_failed: Option<i64>,
    pub sync_branch_wait: Option<i64>,
    pub join_completion: Option<i64>,
    pub lifecycle_event: Option<i64>,
    pub polling_wait: Option<i64>,
    pub polling_interval: Option<i64>,
    pub default: Option<i64>,
    pub max_allowed: Option<i64>,
}
