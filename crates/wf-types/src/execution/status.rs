use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Created,
    Running,
    Paused,
    Stopped,
    Completed,
    Failed,
    Cancelled,
}
