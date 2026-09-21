use crate::report::{FindingLevel, SectionReport};
use crate::trace::Trace;
use crate::traverse::walk;
use crate::views::InterruptionKind;

/// Interruption analysis: distribution by kind, durations and recovery rate.
/// Unrecovered interruptions are errors because they fail the unified check.
pub fn analyze(trace: &Trace) -> SectionReport {
    let mut report = SectionReport::named("interruptions");
    for visit in walk(trace) {
        let Some(interruption) = visit.step.interruption.as_ref() else {
            continue;
        };
        report.count("total", 1);
        report.count(
            match interruption.kind {
                InterruptionKind::Pause => "pause",
                InterruptionKind::Stop => "stop",
                InterruptionKind::Cancel => "cancel",
                InterruptionKind::Timeout => "timeout",
            },
            1,
        );
        if interruption.kind == InterruptionKind::Timeout {
            report.count("timeouts", 1);
        }
        if interruption.recovered {
            report.count("recovered", 1);
        } else {
            report.count("unrecovered", 1);
            report.finding(
                FindingLevel::Error,
                &visit.path,
                format!(
                    "unrecovered {} interruption{}",
                    interruption.kind.label(),
                    interruption
                        .duration_ms
                        .map(|duration| format!(" after {duration}ms"))
                        .unwrap_or_default(),
                ),
                Some(serde_json::Value::Bool(true)),
                Some(serde_json::Value::Bool(false)),
            );
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{InterruptionView, StepRecord, TraceKind, TRACE_SCHEMA_V1};
    use std::collections::HashMap;

    fn trace_with(interruptions: Vec<InterruptionView>) -> Trace {
        let steps = interruptions
            .into_iter()
            .enumerate()
            .map(|(index, interruption)| StepRecord {
                index,
                node_id: "n".to_string(),
                node_name: String::new(),
                node_type: "SCRIPT".to_string(),
                input: serde_json::Value::Null,
                result: serde_json::Value::Null,
                success: true,
                error: None,
                error_kind: None,
                retryable: None,
                recovery_hint: None,
                branch_id: None,
                route_target: None,
                start_time: None,
                end_time: None,
                variable_before: HashMap::new(),
                variable_after: HashMap::new(),
                messages_before: HashMap::new(),
                messages_after: HashMap::new(),
                tool_calls: vec![],
                llm_calls: vec![],
                approval: None,
                visibility: None,
                loop_round: None,
                merge: None,
                interruption: Some(interruption),
                checkpoint: None,
                interaction: None,
                hooks_fired: vec![],
                triggers_seen: vec![],
                exec_id: None,
                parent_exec_id: None,
                root_exec_id: None,
                depth: None,
                result_var: None,
                wait_for_child: None,
                child_timeout_ms: None,
                dialog_anchor: None,
                writeback: None,
                children: vec![],
            })
            .collect();
        Trace {
            schema: TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps,
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        }
    }

    #[test]
    fn recovered_pause_is_clean() {
        let trace = trace_with(vec![InterruptionView {
            kind: InterruptionKind::Pause,
            recovered: true,
            detail: None,
            duration_ms: Some(40),
        }]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("pause"), Some(&1));
        assert_eq!(report.errors(), 0);
    }

    #[test]
    fn unrecovered_timeout_is_an_error() {
        let trace = trace_with(vec![InterruptionView {
            kind: InterruptionKind::Timeout,
            recovered: false,
            detail: Some("deadline exceeded".to_string()),
            duration_ms: Some(5000),
        }]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("timeouts"), Some(&1));
        assert_eq!(report.errors(), 1);
    }
}
