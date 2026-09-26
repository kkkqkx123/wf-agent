use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

/// Terminal outcome of one trigger action run, recorded in the durable
/// ledger. `Abandoned` distinguishes a run that never executed (the
/// listener shut down while it was queued or in flight) from a run that
/// executed and failed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerExecutionOutcome {
    Completed,
    Failed,
    Abandoned,
}

impl TriggerExecutionOutcome {
    /// Canonical wire representation (matches the serde `snake_case` rename).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Abandoned => "abandoned",
        }
    }
}

/// Raised when an outcome string does not match any known wire form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTriggerExecutionOutcome(pub String);

impl std::fmt::Display for UnknownTriggerExecutionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown trigger execution outcome: {}", self.0)
    }
}

impl std::error::Error for UnknownTriggerExecutionOutcome {}

impl FromStr for TriggerExecutionOutcome {
    type Err = UnknownTriggerExecutionOutcome;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "abandoned" => Ok(Self::Abandoned),
            other => Err(UnknownTriggerExecutionOutcome(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerExecutionStorageMetadata {
    pub id: super::super::Id,
    pub trigger_name: String,
    pub trigger_type: String,
    pub event: String,
    pub execution_id: Option<super::super::Id>,
    pub workflow_id: Option<super::super::Id>,
    pub outcome: TriggerExecutionOutcome,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub action_type: Option<String>,
    pub execution_time_ms: i64,
    pub triggered_at: super::super::Timestamp,
}
