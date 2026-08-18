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

impl ExecutionStatus {
    /// Canonical wire representation (matches the serde `snake_case` rename;
    /// do not use `Debug` output, which is PascalCase).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}
