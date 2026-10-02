use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageView {
    pub id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallView {
    pub name: String,
    #[serde(default)]
    pub call_id: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmCallView {
    #[serde(default)]
    pub profile_id: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    /// Reported total; zero means unreported and the analyzer falls back to
    /// prompt plus completion.
    #[serde(default)]
    pub total_tokens: u32,
    #[serde(default)]
    pub reasoning_tokens: Option<u32>,
    #[serde(default)]
    pub cache_read_tokens: Option<u32>,
    #[serde(default)]
    pub cache_write_tokens: Option<u32>,
    #[serde(default)]
    pub total_cost: Option<f64>,
    /// True when the usage is a local estimate rather than provider-reported.
    /// Estimates feed the cost track only and never drive budget decisions.
    #[serde(default)]
    pub estimated: bool,
    #[serde(default)]
    pub content_preview: Option<String>,
    #[serde(default)]
    pub tool_call_count: u32,
    #[serde(default)]
    pub error: Option<String>,
}

impl LlmCallView {
    pub fn effective_total(&self) -> u64 {
        if self.total_tokens > 0 {
            u64::from(self.total_tokens)
        } else {
            u64::from(self.prompt_tokens) + u64::from(self.completion_tokens)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalView {
    pub tool_name: String,
    pub decision: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VisibilityView {
    #[serde(default)]
    pub visible: Vec<String>,
    #[serde(default)]
    pub gated: Vec<String>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub discoverable: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopRoundView {
    pub loop_id: String,
    pub round: u32,
    #[serde(default)]
    pub item: Option<serde_json::Value>,
    #[serde(default)]
    pub failed: bool,
    /// Zero-based position in the iteration sequence, when recorded.
    #[serde(default)]
    pub iteration: Option<u64>,
    #[serde(default)]
    pub max_iterations: Option<u64>,
    /// Cumulative failures for this loop, mirroring the engine counters.
    #[serde(default)]
    pub failures: u32,
    /// Failure policy in effect (for example `continue` or `fail_fast`).
    #[serde(default)]
    pub policy: Option<String>,
    /// True when the round ran after a checkpoint restore.
    #[serde(default)]
    pub resumed: bool,
    /// Nodes of the current round already completed before this record.
    #[serde(default)]
    pub completed_nodes: Vec<String>,
    /// Iteration source: `count` for counted loops, `items` for iterables.
    #[serde(default)]
    pub iterable_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeBranchView {
    pub branch_id: String,
    pub success: bool,
    #[serde(default)]
    pub output: serde_json::Value,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeView {
    pub join_node_id: String,
    #[serde(default)]
    pub branch_count: usize,
    #[serde(default)]
    pub failures_absorbed: usize,
    #[serde(default)]
    pub summary: serde_json::Value,
    /// Join outcome: `success`, `partial` or `failed`. When absent the merge
    /// analyzer derives it from the branch records.
    #[serde(default)]
    pub outcome: Option<String>,
    /// Failure policy in effect (for example `fail_fast` or `threshold`).
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub branches: Vec<MergeBranchView>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteDecisionPoint {
    pub node_id: String,
    #[serde(default)]
    pub branches: Vec<RouteBranch>,
    #[serde(default)]
    pub default_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteBranch {
    pub target_node_id: String,
    #[serde(default)]
    pub expression: Option<String>,
}

/// Trace-level LLM budget. Limits are hard gates; `warn_at` is a fraction
/// (0 to 1) of the token limit that raises an early warning. `context_limit`
/// is the per-request model-window budget compression decisions compare
/// against (0/absent disables compression analysis thresholds).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BudgetView {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub limit_tokens: Option<u64>,
    #[serde(default)]
    pub limit_cost: Option<f64>,
    #[serde(default)]
    pub warn_at: Option<f64>,
    #[serde(default)]
    pub context_limit: Option<u64>,
}

/// One context-compression lifecycle event recorded on a step.
///
/// The engine emits `requested` when a named message array exceeds its
/// context budget (or the provider forces a safety-net request), then the
/// compression service publishes exactly one terminal event for the same
/// `(target_context_id, array_version)`: `completed` (summary or degraded
/// partial window landed), `failed` (retries exhausted, emitter parks for
/// external handling) or `discarded` (stale result dropped after the array
/// moved past the anchor; normal concurrency, emitter continues). A
/// `requested` without a matching terminal event means the emitter blocked
/// until its settle timeout.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CompressionPhase {
    Requested,
    Completed,
    Failed,
    Discarded,
}

impl CompressionPhase {
    pub fn label(self) -> &'static str {
        match self {
            CompressionPhase::Requested => "requested",
            CompressionPhase::Completed => "completed",
            CompressionPhase::Failed => "failed",
            CompressionPhase::Discarded => "discarded",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompressionView {
    pub target_context_id: String,
    pub phase: CompressionPhase,
    /// Array version at emission time (decision-track idempotency anchor).
    #[serde(default)]
    pub array_version: u64,
    /// Estimated tokens of the target array at request time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u64>,
    /// Context budget the array was checked against (0 = unknown budget).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_limit: Option<u64>,
    /// Message count of the target array at request time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_count: Option<usize>,
    /// Estimated token count after compression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_after: Option<u64>,
    /// Recent pre-existing messages kept visible alongside the summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_keep: Option<usize>,
    /// True when the request was forced by a provider context-length error.
    #[serde(default)]
    pub forced: bool,
    /// True when a forced request had no known context budget.
    #[serde(default)]
    pub budget_unknown: bool,
    /// True when the compression signal had no hook handler to take over
    /// (audit event kept, backpressure never anchored, emitter proceeds
    /// without compression and likely hits the provider limit again).
    /// Requested phase only.
    #[serde(default)]
    pub no_taker: bool,
    /// True when the completion is the terminal-failure fallback (a locally
    /// trimmed window without an LLM summary, headed by a degraded notice).
    #[serde(default)]
    pub degraded: bool,
    /// Messages dropped without a summary on a degraded fallback.
    #[serde(default)]
    pub degraded_dropped: u64,
    /// True when the compressed result still exceeds the emission budget.
    #[serde(default)]
    pub still_over_budget: bool,
    /// Summary text extracted from the compressed result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Terminal failure reason (failed phase only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Attempts spent on the failed run (failed phase only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    /// Discard reason (discarded phase only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard_reason: Option<String>,
    /// Current array version that won over the discarded anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_version: Option<u64>,
    /// Run identity stamped by the compression service on terminal events.
    /// Distinct run ids for the same `(target, version)` mark redundant
    /// summary runs whose write-back the version anchor discards. Absent
    /// on old traces predating the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interruption_kind_parses_legacy_timeout_spelling() {
        let kind: InterruptionKind = serde_json::from_str("\"Timeout\"").expect("alias parses");
        assert_eq!(kind, InterruptionKind::Timeout);
    }

    #[test]
    fn llm_total_falls_back_to_prompt_plus_completion() {
        let call = LlmCallView {
            profile_id: "p".to_string(),
            model: None,
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 0,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            total_cost: None,
            estimated: false,
            content_preview: None,
            tool_call_count: 0,
            error: None,
        };
        assert_eq!(call.effective_total(), 15);
    }
}
