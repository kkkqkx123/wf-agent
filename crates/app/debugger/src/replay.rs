use serde::{Deserialize, Serialize};

use crate::model::Trace;
use crate::observe::{all_message_diffs, variable_diffs, MessageDiff, VariableDiff};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepOutcome {
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayOutcome {
    pub steps: Vec<StepOutcome>,
    pub summary: ReplaySummary,
}

pub fn replay_trace(trace: &Trace) -> ReplayOutcome {
    let mut steps = Vec::new();
    let mut summary = ReplaySummary {
        steps: trace.steps.len(),
        ..Default::default()
    };

    for step in &trace.steps {
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
            .is_some_and(|i| i.interruption_type == "timeout" || i.interruption_type == "Timeout")
        {
            summary.timeouts += 1;
        }
        for child in &step.children {
            summary.steps += 1;
            summary.tool_calls += child.tool_calls.len();
            if !child.success {
                summary.failures += 1;
            }
        }
        steps.push(StepOutcome {
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
    use crate::model::{StepRecord, TraceKind};
    use std::collections::HashMap;

    #[test]
    fn counts_failures_and_tool_errors() {
        let trace = Trace {
            schema: crate::model::TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            graph_ref: String::new(),
            initial_variables: HashMap::new(),
            steps: vec![StepRecord {
                index: 0,
                node_id: "n1".to_string(),
                node_name: String::new(),
                node_type: "SCRIPT".to_string(),
                input: serde_json::Value::Null,
                result: serde_json::Value::Null,
                success: false,
                error: Some("boom".to_string()),
                error_kind: None,
                retryable: None,
                recovery_hint: None,
                branch_id: None,
                route_target: None,
                start_time: Some(1),
                end_time: Some(4),
                variable_before: HashMap::new(),
                variable_after: HashMap::new(),
                messages_before: HashMap::new(),
                messages_after: HashMap::new(),
                tool_calls: vec![crate::model::ToolCallView {
                    name: "t".to_string(),
                    call_id: String::new(),
                    arguments: serde_json::Value::Null,
                    result: None,
                    error: Some("e".to_string()),
                    duration_ms: None,
                    success: false,
                }],
                llm_calls: vec![],
                approval: None,
                visibility: None,
                loop_round: None,
                merge: None,
                interruption: None,
                checkpoint: None,
                interaction: None,
                hooks_fired: vec![],
                triggers_seen: vec![],
                children: vec![],
            }],
            assertions: vec![],
            trigger_templates: vec![],
        };
        let outcome = replay_trace(&trace);
        assert_eq!(outcome.summary.failures, 1);
        assert_eq!(outcome.summary.tool_failures, 1);
        assert_eq!(outcome.steps[0].duration_ms, Some(3));
    }
}
