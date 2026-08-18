use serde::{Deserialize, Serialize};

/// When to create a checkpoint around a tool execution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointTiming {
    Before,
    After,
    Both,
}
