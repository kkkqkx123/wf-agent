use serde::{Deserialize, Serialize};

/// Interruption kinds, mirroring the engine signal vocabulary plus the
/// workflow-level cancel and timeout outcomes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InterruptionKind {
    Pause,
    Stop,
    #[serde(alias = "cancelled", alias = "canceled")]
    Cancel,
    #[serde(alias = "Timeout", alias = "timed_out")]
    Timeout,
}

impl InterruptionKind {
    pub fn label(self) -> &'static str {
        match self {
            InterruptionKind::Pause => "pause",
            InterruptionKind::Stop => "stop",
            InterruptionKind::Cancel => "cancel",
            InterruptionKind::Timeout => "timeout",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterruptionView {
    #[serde(rename = "type")]
    pub kind: InterruptionKind,
    #[serde(default)]
    pub recovered: bool,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

/// Checkpoint timings, mirroring the engine strategy variants plus the
/// compression boundary checkpoints (`before_compression` is the
/// pre-compression snapshot, `after_compression` the post-write-back gate).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointTiming {
    BeforeNode,
    AfterNode,
    NodeError,
    WorkflowStart,
    WorkflowEnd,
    WorkflowPause,
    WorkflowCancel,
    WorkflowTimeout,
    BeforeCompression,
    AfterCompression,
}

impl CheckpointTiming {
    pub fn label(self) -> &'static str {
        match self {
            CheckpointTiming::BeforeNode => "before_node",
            CheckpointTiming::AfterNode => "after_node",
            CheckpointTiming::NodeError => "node_error",
            CheckpointTiming::WorkflowStart => "workflow_start",
            CheckpointTiming::WorkflowEnd => "workflow_end",
            CheckpointTiming::WorkflowPause => "workflow_pause",
            CheckpointTiming::WorkflowCancel => "workflow_cancel",
            CheckpointTiming::WorkflowTimeout => "workflow_timeout",
            CheckpointTiming::BeforeCompression => "before_compression",
            CheckpointTiming::AfterCompression => "after_compression",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointSource {
    Policy,
    Hook,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointMark {
    pub timing: CheckpointTiming,
    #[serde(default)]
    pub checkpoint_id: Option<String>,
    #[serde(default)]
    pub source: Option<CheckpointSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InteractionView {
    pub interaction_id: String,
    pub prompt: String,
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub response: Option<serde_json::Value>,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default)]
    pub wait_ms: Option<i64>,
    /// True when the waiter was discarded while still pending.
    #[serde(default)]
    pub dropped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookFireView {
    pub hook_type: String,
    #[serde(default)]
    pub hook_id: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub condition_matched: Option<bool>,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub handler: Option<String>,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub veto_reason: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub gate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerEventView {
    pub template_name: String,
    pub event_type: String,
    #[serde(default)]
    pub event_name: Option<String>,
    #[serde(default)]
    pub matched: bool,
    #[serde(default)]
    pub drop_reason: Option<String>,
    #[serde(default)]
    pub permit: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerTemplateView {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub event_name: Option<String>,
    #[serde(default)]
    pub expression: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub max_triggers: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interruption_kind_parses_legacy_timeout_spelling() {
        let kind: InterruptionKind = serde_json::from_str("\"Timeout\"").expect("alias parses");
        assert_eq!(kind, InterruptionKind::Timeout);
    }
}
