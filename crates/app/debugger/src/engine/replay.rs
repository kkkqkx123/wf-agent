use serde::{Deserialize, Serialize};

use crate::model::observe::{all_message_diffs, variable_diffs, MessageDiff, VariableDiff};
use crate::model::trace::Trace;
use crate::model::traverse::walk;
use crate::model::views_runtime::InterruptionKind;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepOutcome {
    /// Walk path of the step, unique within the trace.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub depth: usize,
    pub index: usize,
    pub node_id: String,
    pub node_type: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_target: Option<String>,
    pub duration_ms: Option<i64>,
    pub variable_diffs: Vec<VariableDiff>,
    pub message_diffs: Vec<MessageDiff>,
    pub tool_failures: usize,
    pub hook_vetoes: usize,
    pub trigger_matches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ReplaySummary {
    pub steps: usize,
    pub failures: usize,
    pub tool_calls: usize,
    pub tool_failures: usize,
    pub hook_vetoes: usize,
    pub trigger_matches: usize,
    pub pending_interactions: usize,
    pub timeouts: usize,
    #[serde(default)]
    pub loop_rounds: usize,
    #[serde(default)]
    pub merges: usize,
    #[serde(default)]
    pub checkpoints: usize,
    #[serde(default)]
    pub llm_calls: usize,
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub compression_requested: usize,
    #[serde(default)]
    pub compression_routed: usize,
    #[serde(default)]
    pub compression_completed: usize,
    #[serde(default)]
    pub compression_failed: usize,
    #[serde(default)]
    pub compression_discarded: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayOutcome {
    pub steps: Vec<StepOutcome>,
    pub summary: ReplaySummary,
}

pub fn replay_trace(trace: &Trace) -> ReplayOutcome {
    let mut steps = Vec::new();
    let mut summary = ReplaySummary::default();

    for visit in walk(trace) {
        let step = visit.step;
        let variable_diffs = variable_diffs(&step.variable_before, &step.variable_after);
        let message_diffs = all_message_diffs(&step.messages_before, &step.messages_after);
        let tool_failures = step.tool_calls.iter().filter(|t| !t.success).count();
        let hook_vetoes = step
            .hooks_fired
            .iter()
            .filter(|h| h.outcome == "veto" || h.veto_reason.is_some())
            .count();
        let trigger_matches = step.triggers_seen.iter().filter(|t| t.matched).count();
        let duration_ms = match (step.start_time, step.end_time) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        };
        summary.steps += 1;
        if !step.success {
            summary.failures += 1;
        }
        summary.tool_calls += step.tool_calls.len();
        summary.tool_failures += tool_failures;
        summary.hook_vetoes += hook_vetoes;
        summary.trigger_matches += trigger_matches;
        if step.interaction.as_ref().is_some_and(|i| i.pending) {
            summary.pending_interactions += 1;
        }
        if step
            .interruption
            .as_ref()
            .is_some_and(|i| i.kind == InterruptionKind::Timeout)
        {
            summary.timeouts += 1;
        }
        if step.loop_round.is_some() {
            summary.loop_rounds += 1;
        }
        if step.merge.is_some() {
            summary.merges += 1;
        }
        if step.checkpoint.is_some() {
            summary.checkpoints += 1;
        }
        summary.llm_calls += step.llm_calls.len();
        for call in &step.llm_calls {
            summary.prompt_tokens += u64::from(call.prompt_tokens);
            summary.completion_tokens += u64::from(call.completion_tokens);
            summary.total_tokens += call.effective_total();
        }
        for compression in &step.compressions {
            match compression.phase {
                crate::model::CompressionPhase::Requested => {
                    summary.compression_requested += 1;
                }
                crate::model::CompressionPhase::Routed => {
                    summary.compression_routed += 1;
                }
                crate::model::CompressionPhase::Completed => {
                    summary.compression_completed += 1;
                }
                crate::model::CompressionPhase::Failed => {
                    summary.compression_failed += 1;
                }
                crate::model::CompressionPhase::Discarded => {
                    summary.compression_discarded += 1;
                }
            }
        }
        steps.push(StepOutcome {
            path: visit.path,
            depth: visit.depth,
            index: step.index,
            node_id: step.node_id.clone(),
            node_type: step.node_type.clone(),
            success: step.success,
            error: step.error.clone(),
            route_target: step.route_target.clone(),
            duration_ms,
            variable_diffs,
            message_diffs,
            tool_failures,
            hook_vetoes,
            trigger_matches,
        });
    }

    ReplayOutcome { steps, summary }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{StepRecord, ToolCallView, TraceKind};

    #[test]
    fn counts_failures_and_tool_errors() {
        let trace = Trace {
            schema: crate::model::TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            steps: vec![StepRecord {
                index: 0,
                node_id: "n1".to_string(),
                node_type: "SCRIPT".to_string(),
                success: false,
                error: Some("boom".to_string()),
                start_time: Some(1),
                end_time: Some(4),
                tool_calls: vec![ToolCallView {
                    name: "t".to_string(),
                    error: Some("e".to_string()),
                    success: false,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let outcome = replay_trace(&trace);
        assert_eq!(outcome.summary.failures, 1);
        assert_eq!(outcome.summary.tool_failures, 1);
        assert_eq!(outcome.steps[0].duration_ms, Some(3));
        assert_eq!(outcome.steps[0].path, "0");
    }
}
