use std::collections::HashSet;

use crate::report::{FindingLevel, SectionReport};
use crate::trace::Trace;
use crate::traverse::walk;

/// Loop analysis: group rounds by loop id, surface failures, absorptions
/// and restores so nested loops and failure policies stay explainable.
pub fn analyze(trace: &Trace) -> SectionReport {
    let mut report = SectionReport::named("loops");
    let mut loop_ids: HashSet<&str> = HashSet::new();
    for visit in walk(trace) {
        let Some(round) = visit.step.loop_round.as_ref() else {
            continue;
        };
        loop_ids.insert(round.loop_id.as_str());
        report.count("rounds", 1);
        if round.failed {
            report.count("failed_rounds", 1);
            let absorbed = round
                .policy
                .as_deref()
                .is_some_and(|policy| policy != "fail_fast");
            report.finding(
                if absorbed {
                    FindingLevel::Warning
                } else {
                    FindingLevel::Error
                },
                &visit.path,
                format!(
                    "loop '{}' round {} failed (policy {})",
                    round.loop_id,
                    round.round,
                    round.policy.clone().unwrap_or_else(|| "unset".to_string()),
                ),
                None,
                visit.step.error.clone().map(serde_json::Value::String),
            );
        }
        if round.resumed {
            report.count("resumed_rounds", 1);
            report.finding(
                FindingLevel::Info,
                &visit.path,
                format!(
                    "loop '{}' round {} resumed after restore with {} completed node(s)",
                    round.loop_id,
                    round.round,
                    round.completed_nodes.len(),
                ),
                None,
                None,
            );
        }
        if let Some(max) = round.max_iterations {
            if u64::from(round.round) + 1 >= max {
                report.count("limit_rounds", 1);
                report.finding(
                    FindingLevel::Warning,
                    &visit.path,
                    format!(
                        "loop '{}' round {} reached the configured limit of {max} iterations",
                        round.loop_id, round.round,
                    ),
                    Some(serde_json::Value::from(max)),
                    Some(serde_json::Value::from(round.round)),
                );
            }
        }
    }
    report.count("loops", loop_ids.len() as u64);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LoopRoundView, StepRecord, TraceKind, TRACE_SCHEMA_V1};
    use std::collections::HashMap;

    fn trace_with_rounds(rounds: Vec<LoopRoundView>) -> Trace {
        let steps = rounds
            .into_iter()
            .enumerate()
            .map(|(index, loop_round)| StepRecord {
                index,
                node_id: "loop-node".to_string(),
                node_name: String::new(),
                node_type: "LOOP".to_string(),
                input: serde_json::Value::Null,
                result: serde_json::Value::Null,
                success: !loop_round.failed,
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
                loop_round: Some(loop_round),
                merge: None,
                interruption: None,
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

    fn round(loop_id: &str, round: u32, failed: bool) -> LoopRoundView {
        LoopRoundView {
            loop_id: loop_id.to_string(),
            round,
            item: None,
            failed,
            iteration: Some(u64::from(round)),
            max_iterations: Some(3),
            failures: u32::from(failed),
            policy: Some("continue".to_string()),
            resumed: false,
            completed_nodes: vec![],
            iterable_kind: Some("count".to_string()),
        }
    }

    #[test]
    fn counts_rounds_and_flags_failures() {
        let trace = trace_with_rounds(vec![round("l1", 0, false), round("l1", 1, true)]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("rounds"), Some(&2));
        assert_eq!(report.counts.get("failed_rounds"), Some(&1));
        assert_eq!(report.counts.get("loops"), Some(&1));
        assert!(report.errors() == 0);
    }

    #[test]
    fn fail_fast_failure_is_an_error() {
        let mut failed = round("l1", 0, true);
        failed.policy = Some("fail_fast".to_string());
        let trace = trace_with_rounds(vec![failed]);
        let report = analyze(&trace);
        assert_eq!(report.errors(), 1);
    }
}
