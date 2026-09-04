use serde::{Deserialize, Serialize};
use std::str::FromStr;

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
    Timeout,
}

/// Raised when a status string does not match any known wire form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownExecutionStatus(pub String);

impl std::fmt::Display for UnknownExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown execution status: {}", self.0)
    }
}

impl std::error::Error for UnknownExecutionStatus {}

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
            Self::Timeout => "timeout",
        }
    }

    /// Lenient parse of the canonical wire form. Unknown input resolves to
    /// `Running` so that legacy or partially populated records stay usable
    /// instead of being discarded. Matching is case-insensitive because
    /// status values cross process and storage boundaries that have not
    /// always normalized casing.
    pub fn from_wire(status: &str) -> Self {
        Self::from_str(status).unwrap_or(Self::Running)
    }
}

impl FromStr for ExecutionStatus {
    type Err = UnknownExecutionStatus;

    fn from_str(status: &str) -> Result<Self, Self::Err> {
        match status.trim().to_ascii_lowercase().as_str() {
            "created" => Ok(Self::Created),
            "running" => Ok(Self::Running),
            "paused" => Ok(Self::Paused),
            "stopped" => Ok(Self::Stopped),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "timeout" => Ok(Self::Timeout),
            other => Err(UnknownExecutionStatus(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_round_trip_covers_every_variant() {
        let all = [
            ExecutionStatus::Created,
            ExecutionStatus::Running,
            ExecutionStatus::Paused,
            ExecutionStatus::Stopped,
            ExecutionStatus::Completed,
            ExecutionStatus::Failed,
            ExecutionStatus::Cancelled,
            ExecutionStatus::Timeout,
        ];
        for status in all {
            let parsed = ExecutionStatus::from_str(status.as_str())
                .expect("as_str output must be parseable");
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn timeout_is_recognized() {
        assert_eq!(
            ExecutionStatus::from_str("timeout").expect("timeout is a known status"),
            ExecutionStatus::Timeout
        );
        assert_eq!(ExecutionStatus::from_wire("timeout"), ExecutionStatus::Timeout);
    }

    #[test]
    fn parsing_is_case_and_whitespace_insensitive() {
        assert_eq!(ExecutionStatus::from_wire("Running"), ExecutionStatus::Running);
        assert_eq!(
            ExecutionStatus::from_wire(" TIMEOUT "),
            ExecutionStatus::Timeout
        );
    }

    #[test]
    fn unknown_status_falls_back_to_running() {
        assert_eq!(
            ExecutionStatus::from_wire("not-a-status"),
            ExecutionStatus::Running
        );
        assert!(ExecutionStatus::from_str("not-a-status").is_err());
    }
}
