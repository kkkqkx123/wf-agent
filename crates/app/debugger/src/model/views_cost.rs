use serde::{Deserialize, Serialize};

use crate::model::util::cap_payload_text;

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
/// context budget (or the provider forces a safety-net request), the builtin
/// adapter publishes `routed` when it hands the snapshot-carrying copy to the
/// listener (direct-mode traces have no `routed` entry), then the compression
/// service publishes exactly one terminal event for the same
/// `(target_context_id, array_version)`: `completed` (summary or degraded
/// partial window landed), `failed` (retries exhausted, emitter parks for
/// external handling) or `discarded` (stale result dropped after the array
/// moved past the anchor; normal concurrency, emitter continues). A
/// `requested` without a matching terminal event means the emitter blocked
/// until its settle timeout; a `routed` without a terminal means the handoff
/// never ran (listener down or claim lost to a duplicate).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CompressionPhase {
    Requested,
    Routed,
    Completed,
    Failed,
    Discarded,
}

impl CompressionPhase {
    pub fn label(self) -> &'static str {
        match self {
            CompressionPhase::Requested => "requested",
            CompressionPhase::Routed => "routed",
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

impl CompressionView {
    /// Build the trailing detail string for text rendering, carrying token
    /// counts, degradation notes, phase markers, run identity and failure
    /// reasons in a single human-readable fragment.
    pub fn summary_suffix(&self) -> String {
        let mut parts = Vec::new();
        if let Some(tokens) = self.tokens_used {
            parts.push(format!(
                "tokens={}/{}",
                tokens,
                self.token_limit
                    .map(|limit| limit.to_string())
                    .unwrap_or_else(|| "?".to_string())
            ));
        }
        if let Some(tokens_after) = self.tokens_after {
            parts.push(format!("after={tokens_after}"));
        }
        if self.forced {
            parts.push("forced".to_string());
        }
        if self.no_taker {
            parts.push("no_taker".to_string());
        }
        if self.degraded {
            if self.degraded_dropped > 0 {
                parts.push(format!(
                    "degraded(dropped={})",
                    self.degraded_dropped
                ));
            } else {
                parts.push("degraded".to_string());
            }
        }
        if self.still_over_budget {
            parts.push("still_over_budget".to_string());
        }
        if self.phase == CompressionPhase::Discarded {
            parts.push("discarded".to_string());
        }
        if self.phase == CompressionPhase::Routed {
            parts.push("routed".to_string());
        }
        if let Some(run_id) = self.run_id.as_deref() {
            parts.push(format!("run={}", cap_payload_text(run_id)));
        }
        if let Some(reason) = self.discard_reason.as_deref() {
            parts.push(format!("reason={}", cap_payload_text(reason)));
        }
        if let Some(error) = self.error.as_deref() {
            parts.push(format!("error={}", cap_payload_text(error)));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!(" {}", parts.join(" "))
        }
    }
}
